use log::info;
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Instant,
};

#[derive(Debug, Clone, Copy)]
pub struct TicketPolicy {
    pub max_global: usize,
    pub max_per_site: usize,
    pub rate_limit_wait_time: Duration,
}

struct TicketPartition {
    // Needed to be arced to own a permit, apparently
    lock: Arc<tokio::sync::Semaphore>,
    locked_till: Option<Instant>,
    policy: Arc<TicketPolicy>,
}

pub struct Ticketer<Origin: Clone + Hash + Eq> {
    global_lock: Arc<Semaphore>,
    // Fine to use a mutex, should be very little contention
    state: Mutex<HashMap<Origin, TicketPartition>>,
    policy: Arc<TicketPolicy>,
}

impl<Origin: Clone + Hash + Eq> Debug for Ticketer<Origin> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ticketer")
    }
}

impl<Origin: Clone + Hash + Eq> Ticketer<Origin> {
    pub fn new(policy: &TicketPolicy) -> Self {
        Ticketer {
            global_lock: Arc::new(Semaphore::new(policy.max_global)),
            state: Default::default(),
            policy: Arc::new(policy.clone()),
        }
    }

    fn default_origin_details(&self) -> TicketPartition {
        TicketPartition {
            lock: Arc::new(Semaphore::new(self.policy.max_per_site)),
            locked_till: None,
            policy: self.policy.clone(),
        }
    }

    pub fn mark_origin_locked(&self, origin: &Origin) {
        let wait_time = self.policy.rate_limit_wait_time;
        info!("Rate limit exceeded, waiting for {:?}", wait_time);
        let wait_till = Instant::now() + wait_time;
        let mut lock = self.state.lock().unwrap();
        lock.get_mut(origin).unwrap().locked_till = Some(wait_till);
    }

    fn get_origin_locked_till(&self, origin: &Origin) -> Instant {
        // We won't try to deal with lock poisoning
        let mut guard = self.state.lock().unwrap();
        // TODO-OPTIMIZE away the clone
        guard
            .entry(origin.clone())
            .or_insert(self.default_origin_details())
            .locked_till
            .unwrap_or(Instant::now())
    }

    fn get_origin_lock(&self, origin: &Origin) -> Arc<Semaphore> {
        // We won't try to deal with lock poisoning
        let mut guard = self.state.lock().unwrap();
        // TODO-OPTIMIZE away the clone
        guard
            .entry(origin.clone())
            .or_insert(self.default_origin_details())
            .lock
            .clone()
    }

    #[cfg(test)]
    pub fn can_get_ticket(&self, origin: &Origin) -> bool {
        if self.global_lock.available_permits() == 0 {
            return false;
        }
        self.get_origin_lock(origin).available_permits() > 0
    }

    pub async fn get_ticket(&self, origin: &Origin) -> Ticket {
        use tokio::time::sleep_until;
        // We assume that the ticketer semaphore will never be closed, so it is safe to unwrap
        let mut _local_permit = None;
        loop {
            _local_permit = Some(self.get_origin_lock(origin).acquire_owned().await.unwrap());
            let wait_till = self.get_origin_locked_till(origin);
            if Instant::now() >= wait_till {
                break;
            } else {
                _local_permit = None;
                sleep_until(wait_till).await;
            }
        }
        let _global_permit = self.global_lock.clone().acquire_owned().await.unwrap();
        Ticket {
            _global_permit,
            _local_permit: _local_permit.unwrap(),
        }
    }
}

pub struct Ticket {
    _global_permit: OwnedSemaphorePermit,
    // Local permit should be dropped after global permit, enforced by RFC 1857
    _local_permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use tokio::{join, sync::Barrier};

    #[tokio::test]
    async fn test_ticket_blocking() {
        let policy = TicketPolicy {
            max_global: 2,
            max_per_site: 1,
            rate_limit_wait_time: Duration::new(60, 0),
        };
        let ticketer = Ticketer::new(&policy);
        let origin1 = "foo".to_string();
        let origin2 = "bar".to_string();
        let origin3 = "bar".to_string();
        for _ in 0..100 {
            let b1 = Barrier::new(2);
            let b2 = Barrier::new(3);
            let b3 = Barrier::new(2);
            let v = AtomicUsize::new(0);

            let f1 = || async {
                assert!(ticketer.can_get_ticket(&origin1));
                let _x = ticketer.get_ticket(&origin1).await;
                assert!(!ticketer.can_get_ticket(&origin1));
                let _y = ticketer.get_ticket(&origin2).await;
                assert!(!ticketer.can_get_ticket(&origin3));
                v.store(1, Ordering::SeqCst);
                b1.wait().await;
                b2.wait().await;
                b3.wait().await;
            };
            let f2 = || async {
                b1.wait().await;
                assert_eq!(v.load(Ordering::SeqCst), 1);
                v.store(2, Ordering::SeqCst);
                b2.wait().await;
            };
            let f3 = || async {
                b2.wait().await;
                assert!(!ticketer.can_get_ticket(&origin2));
                b3.wait().await;
                let _y = ticketer.get_ticket(&origin2).await;
                assert_eq!(v.load(Ordering::SeqCst), 2);
                v.store(3, Ordering::SeqCst);
            };

            join!(f1(), f2(), f3());
            assert_eq!(v.load(Ordering::SeqCst), 3);
        }
    }
    #[tokio::test]
    async fn test_ticket_local_multi() {
        let policy = TicketPolicy {
            max_global: 3,
            max_per_site: 2,
            rate_limit_wait_time: Duration::new(0, 5_000_000),
        };
        let ticketer = Ticketer::new(&policy);
        let origin = "origin".to_string();
        {
            let _t1 = ticketer.get_ticket(&origin).await;
            let _t2 = ticketer.get_ticket(&origin).await;
            assert!(!ticketer.can_get_ticket(&origin));
        }
        assert!(ticketer.can_get_ticket(&origin));
    }

    #[tokio::test]
    async fn test_ticket_locks_on_rate_limit() {
        let policy = TicketPolicy {
            max_global: 1,
            max_per_site: 1,
            rate_limit_wait_time: Duration::new(0, 5_000_000),
        };
        let ticketer = Ticketer::new(&policy);
        let start = Instant::now();
        let origin = "origin".to_string();
        ticketer.ensure_exists(&origin);
        ticketer.mark_origin_locked(&origin);
        {
            ticketer.get_ticket(&origin).await;
        }
        assert!(Instant::now() > start + policy.rate_limit_wait_time);
        let start = Instant::now();
        let offset = Duration::new(0, 1_500_000);
        let f1 = || async {
            tokio::time::sleep_until(Instant::now() + offset).await;
            ticketer.mark_origin_locked(&origin);
        };
        let f2 = || async {
            ticketer.get_ticket(&origin).await;
            let now = Instant::now();
            assert!(now > start + offset + policy.rate_limit_wait_time);
            assert!(now < start + offset + 2 * policy.rate_limit_wait_time);
        };
        ticketer.mark_origin_locked(&origin);
        join!(f1(), f2());
    }
}

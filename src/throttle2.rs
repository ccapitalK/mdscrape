use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::{OwnedSemaphorePermit, Semaphore}, time::Instant};

type Origin = String;

#[derive(Debug, Clone, Copy)]
struct TicketPolicy {
    pub max_global: usize,
    pub max_per_site: usize,
    pub rate_limit_wait_time: Duration,
}

struct OriginDetails {
    // Needed to be arced to own a permit, apparently
    lock: Arc<tokio::sync::Semaphore>,
    locked_till: Option<Instant>,
    policy: Arc<TicketPolicy>,
}

struct Ticketer {
    global_lock: Arc<Semaphore>,
    state: Mutex<HashMap<Origin, OriginDetails>>,
    policy: Arc<TicketPolicy>,
}

impl Ticketer {
    pub fn new(policy: &TicketPolicy) -> Self {
        Ticketer {
            global_lock: Arc::new(Semaphore::new(policy.max_global)),
            state: Default::default(),
            policy: Arc::new(policy.clone()),
        }
    }

    fn default_origin_details(&self) -> OriginDetails {
        OriginDetails {
            lock: Arc::new(Semaphore::new(self.policy.max_per_site)),
            locked_till: None,
            policy: self.policy.clone(),
        }
    }

    fn mark_origin_locked(&self, origin: &Origin) {
        let wait_till = Instant::now() + self.policy.rate_limit_wait_time;
        let mut lock = self.state.lock().unwrap();
        lock.get_mut(origin).unwrap().locked_till = Some(wait_till);
    }

    fn get_origin_locked_till(&self, origin: &Origin) -> Instant {
        // We won't try to deal with lock poisoning
        let mut guard = self.state.lock().unwrap();
        // TODO-OPTIMIZE
        guard.entry(origin.clone()).or_insert(self.default_origin_details()).locked_till.unwrap_or(Instant::now())
    }

    fn ensure_exists<'ticketer>(&'ticketer self, origin: &Origin) {
        // We won't try to deal with lock poisoning
        let mut guard = self.state.lock().unwrap();
        // TODO-OPTIMIZE
        guard.entry(origin.clone()).or_insert(self.default_origin_details());
    }

    fn get_origin_lock(&self, origin: &Origin) -> Arc<Semaphore> {
        // We won't try to deal with lock poisoning
        let mut guard = self.state.lock().unwrap();
        // TODO-OPTIMIZE
        guard.entry(origin.clone()).or_insert(self.default_origin_details()).lock.clone()
    }

    pub fn can_get_ticket(&self, origin: &Origin) -> bool {
        if self.global_lock.available_permits() == 0 {
            return false;
        }
        self.get_origin_lock(origin).available_permits() > 0
    }

    pub async fn get_ticket(&self, origin: &Origin) -> Ticket {
        use tokio::time::sleep_until;
        let mut wait_till = self.get_origin_locked_till(origin);
        while Instant::now() < wait_till {
            sleep_until(wait_till).await;
            wait_till = self.get_origin_locked_till(origin);
        }
        let local_permit = self.get_origin_lock(origin)
            .acquire_owned()
            .await
            .unwrap(); // We assume that the ticketer semaphore will never be close
        let global_permit = self.global_lock.clone().acquire_owned().await.unwrap();
        Ticket { global_permit, local_permit }
    }
}

struct Ticket {
    global_permit: OwnedSemaphorePermit,
    // Local permit should be dropped after global permit, enforced by RFC 1857
    local_permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{time::Duration, sync::atomic::{AtomicUsize, Ordering}};
    use tokio::{join, sync::Barrier};

    #[tokio::test]
    async fn test_nticket_blocking() {
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
    async fn test_nticket_locks_on_rate_limit() {
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
        let offset = Duration::new(0, 1_500_000);
        let f1 = || async {
            tokio::time::sleep_until(Instant::now() + offset).await;
            ticketer.mark_origin_locked(&origin);
        };
        let f2 = || async {
            ticketer.get_ticket(&origin).await;
            assert!(Instant::now() > start + offset + policy.rate_limit_wait_time);
        };
        ticketer.mark_origin_locked(&origin);
        join!(f1(), f2());
    }
}

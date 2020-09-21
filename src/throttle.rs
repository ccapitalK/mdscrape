use core::{future::Future, pin::Pin};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;
use std::task::{Context, Poll, Waker};

pub struct Ticket<'ticketer, K: Clone + Debug + Eq + Hash + PartialEq> {
    key: K,
    ticketer: &'ticketer RefCell<Ticketer<K>>,
}

impl<'ticketer, K> Drop for Ticket<'ticketer, K>
where
    K: Clone + Debug + Eq + Hash + PartialEq,
{
    fn drop(&mut self) {
        self.ticketer.borrow_mut().release(&self.key);
    }
}

struct QueuedTask<K: Clone + Eq + PartialEq> {
    key: K,
    waker: Waker,
    high_priority: bool,
}

// Tries to vend tickets in roughly FIFO order
// FIXME: Optimize this please, everything's linear :(
pub struct Ticketer<K: Clone + Debug + Eq + Hash + PartialEq> {
    tickets: HashMap<K, usize>,
    per_origin_threshold: usize,
    global_count: usize,
    global_threshold: usize,
    parked_tasks: VecDeque<QueuedTask<K>>,
}

impl<K> Ticketer<K>
where
    K: Clone + Debug + Eq + Hash + PartialEq,
{
    pub fn new(per_origin_threshold: usize, global_threshold: usize) -> Self {
        // Limits, to not accidentally DOS (or more likely get ip-banned by) mangadex
        assert!(per_origin_threshold > 0 && per_origin_threshold <= 6);
        assert!(global_threshold > 0 && global_threshold <= 30);
        Ticketer {
            per_origin_threshold,
            global_threshold,
            global_count: 0,
            tickets: Default::default(),
            parked_tasks: Default::default(),
        }
    }
    fn try_acquire(&mut self, key: &K) -> bool {
        if self.global_count < self.global_threshold {
            if let Some(current_count) = self.tickets.get_mut(key) {
                if *current_count < self.per_origin_threshold {
                    *current_count += 1;
                    self.global_count += 1;
                    true
                } else {
                    false
                }
            } else {
                self.tickets.insert(key.clone(), 1usize);
                self.global_count += 1;
                true
            }
        } else {
            false
        }
    }
    fn release(&mut self, key: &K) {
        assert!(self.global_count > 0);
        self.global_count -= 1;
        let current_count = *self.tickets.get(key).expect("Tried to double free ticket");
        if current_count == 1 {
            self.tickets.remove(key);
        } else {
            self.tickets.insert(key.clone(), current_count - 1);
        }
        self.wake_one()
    }
    fn park(&mut self, key: &K, waker: &Waker, high_priority: bool) {
        let queued_task = QueuedTask {
            key: key.clone(),
            waker: waker.clone(),
            high_priority,
        };
        if high_priority {
            self.parked_tasks.push_front(queued_task);
        } else {
            self.parked_tasks.push_back(queued_task);
        }
    }
    fn wake_one(&mut self) {
        for i in 0..self.parked_tasks.len() {
            let key = &self.parked_tasks[i].key;
            if self.tickets.get(&key).filter(|v| **v == self.per_origin_threshold) == None {
                let task = if self.parked_tasks[i].high_priority {
                    self.parked_tasks.swap_remove_front(i).unwrap()
                } else {
                    self.parked_tasks.swap_remove_back(i).unwrap()
                };
                task.waker.wake();
                return;
            }
        }
    }
}

pub struct TicketFuture<'ticketer, K>
where
    K: Clone + Debug + Eq + Hash + PartialEq,
{
    key: &'ticketer K,
    ticketer: &'ticketer RefCell<Ticketer<K>>,
    high_priority: bool,
}

impl<'ticketer, K> TicketFuture<'ticketer, K>
where
    K: Clone + Debug + Eq + Hash + PartialEq,
{
    pub fn new(key: &'ticketer K, ticketer: &'ticketer RefCell<Ticketer<K>>, high_priority: bool) -> Self {
        TicketFuture {
            key,
            ticketer,
            high_priority,
        }
    }
}

impl<'ticketer, K> Future for TicketFuture<'ticketer, K>
where
    K: Clone + Debug + Eq + Hash + PartialEq,
{
    type Output = Ticket<'ticketer, K>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut ticketer = self.ticketer.borrow_mut();
        if ticketer.try_acquire(self.key) {
            return Poll::Ready(Ticket {
                key: self.key.clone(),
                ticketer: self.ticketer,
            });
        } else {
            ticketer.park(self.key, cx.waker(), self.high_priority);
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod test {
    use crate::throttle::{TicketFuture, Ticketer};
    use std::cell::RefCell;
    #[tokio::test]
    async fn test_throttle_ticketing() {
        let ticketer = RefCell::new(Ticketer::new(2, 3));
        {
            let _ticket1 = TicketFuture::new(&"a", &ticketer).await;
            {
                let _ticket2 = TicketFuture::new(&"a", &ticketer).await;
                {
                    assert!(ticketer.borrow_mut().try_acquire(&"a") == false);
                }
                let _ticket3 = TicketFuture::new(&"b", &ticketer).await;
                {
                    assert!(ticketer.borrow_mut().try_acquire(&"a") == false);
                    assert!(ticketer.borrow_mut().try_acquire(&"b") == false);
                    assert!(ticketer.borrow_mut().try_acquire(&"c") == false);
                    assert!(ticketer.borrow().global_count == 3);
                    assert!(ticketer.borrow().tickets.len() == 2);
                }
            }
            let _ticket4 = TicketFuture::new(&"c", &ticketer).await;
            let _ticket4 = TicketFuture::new(&"c", &ticketer).await;
            {
                assert!(ticketer.borrow_mut().try_acquire(&"a") == false);
                assert!(ticketer.borrow_mut().try_acquire(&"b") == false);
            }
        }
        assert!(ticketer.borrow().global_count == 0);
        assert!(ticketer.borrow().tickets.len() == 0);
    }
}

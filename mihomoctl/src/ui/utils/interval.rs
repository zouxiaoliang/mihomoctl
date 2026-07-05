use std::{
    thread::sleep,
    time::{Duration, Instant},
};

pub struct Interval {
    interval: Duration,
    deadline: Option<Instant>,
}

impl Interval {
    pub fn every(interval: Duration) -> Self {
        Self {
            interval,
            deadline: None,
        }
    }

    pub fn next_tick(&mut self) -> Duration {
        let now = Instant::now();
        if self.deadline.is_none() {
            self.deadline = Some(now + self.interval)
        }
        let deadline = self.deadline.unwrap();
        if now > deadline {
            let mut point = deadline;
            loop {
                point += self.interval;
                if point > now {
                    break point - now;
                }
            }
        } else {
            deadline - now
        }
    }

    pub fn tick(&mut self) {
        sleep(self.next_tick())
    }
}

#[test]
fn test_interval() {
    let mut interval = Interval::every(Duration::from_millis(100));
    let first = interval.next_tick();
    assert!(first <= Duration::from_millis(100));
    assert!(first >= Duration::from_millis(90));
    sleep(Duration::from_millis(50));
    let second = interval.next_tick();
    assert!(second <= Duration::from_millis(60));
    assert!(second >= Duration::from_millis(30));
}

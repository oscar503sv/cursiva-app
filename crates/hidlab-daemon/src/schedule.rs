//! When the main loop probes, applies and checks again (requirements §7 and
//! Milestone 4). Pure: the caller passes the time, so tests need no clock.
//!
//! After an event the daemon pings the mouse every [`RETRY`] until it
//! answers (at most [`READY_BUDGET`]), applies and verifies, then verifies
//! again [`SECOND_CHECK_DELAY`] later to catch anything that overwrote the
//! settings. All of it is measured from the event.

use std::time::{Duration, Instant};

use crate::api::Trigger;

pub const RETRY: Duration = Duration::from_millis(200);
pub const READY_BUDGET: Duration = Duration::from_secs(5);
/// 1 s: inside the 1–2 s the requirements ask for, and short enough to keep
/// the whole sequence within the 2 s target of §7.
pub const SECOND_CHECK_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    WaitReady {
        trigger: Trigger,
        event_at: Instant,
        next_try: Instant,
    },
    SecondCheck {
        event_at: Instant,
        due: Instant,
    },
}

/// Work the loop must do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Ping; if the mouse answers, apply and call [`Scheduler::applied`].
    Probe {
        trigger: Trigger,
        event_at: Instant,
    },
    SecondCheck {
        event_at: Instant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheduler {
    phase: Phase,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self { phase: Phase::Idle }
    }
}

impl Scheduler {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The mouse appeared (udev `add`) or the daemon started: always wins.
    pub fn event(&mut self, trigger: Trigger, now: Instant) {
        self.phase = Phase::WaitReady {
            trigger,
            event_at: now,
            next_try: now,
        };
    }

    /// The system woke up. A connect in progress is not interrupted: it
    /// already ends with a full apply.
    pub fn resumed(&mut self, now: Instant) {
        if !matches!(
            self.phase,
            Phase::WaitReady {
                trigger: Trigger::Connect | Trigger::Start,
                ..
            }
        ) {
            self.event(Trigger::Resume, now);
        }
    }

    pub fn removed(&mut self) {
        self.phase = Phase::Idle;
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        match self.phase {
            Phase::Idle => None,
            Phase::WaitReady { next_try, .. } => Some(next_try),
            Phase::SecondCheck { due, .. } => Some(due),
        }
    }

    pub fn due(&self, now: Instant) -> Option<Action> {
        match self.phase {
            Phase::WaitReady {
                trigger,
                event_at,
                next_try,
            } if next_try <= now => Some(Action::Probe { trigger, event_at }),
            Phase::SecondCheck { event_at, due } if due <= now => {
                Some(Action::SecondCheck { event_at })
            }
            _ => None,
        }
    }

    /// The ping failed. Returns `false` when the budget is spent and the
    /// daemon gives up until the next event.
    pub fn probe_failed(&mut self, now: Instant) -> bool {
        let Phase::WaitReady {
            trigger, event_at, ..
        } = self.phase
        else {
            return false;
        };
        if now.duration_since(event_at) >= READY_BUDGET {
            self.phase = Phase::Idle;
            return false;
        }
        self.phase = Phase::WaitReady {
            trigger,
            event_at,
            next_try: now + RETRY,
        };
        true
    }

    /// The first apply finished.
    pub fn applied(&mut self, event_at: Instant, now: Instant) {
        self.phase = Phase::SecondCheck {
            event_at,
            due: now + SECOND_CHECK_DELAY,
        };
    }

    pub fn done(&mut self) {
        self.phase = Phase::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_probe_apply_then_check_again() {
        let t0 = Instant::now();
        let mut s = Scheduler::default();
        assert_eq!(s.next_deadline(), None);
        s.event(Trigger::Connect, t0);
        assert_eq!(
            s.due(t0),
            Some(Action::Probe {
                trigger: Trigger::Connect,
                event_at: t0
            })
        );
        // Not ready yet: retry later, measured from the same event.
        assert!(s.probe_failed(t0 + Duration::from_millis(250)));
        assert_eq!(s.due(t0 + Duration::from_millis(300)), None);
        let retry = t0 + Duration::from_millis(450);
        assert!(matches!(s.due(retry), Some(Action::Probe { event_at, .. }) if event_at == t0));
        s.applied(t0, t0 + Duration::from_millis(600));
        assert_eq!(s.due(t0 + Duration::from_millis(1500)), None);
        assert_eq!(
            s.due(t0 + Duration::from_millis(1600)),
            Some(Action::SecondCheck { event_at: t0 })
        );
        s.done();
        assert_eq!(s.phase(), Phase::Idle);
    }

    #[test]
    fn gives_up_after_the_budget() {
        let t0 = Instant::now();
        let mut s = Scheduler::default();
        s.event(Trigger::Connect, t0);
        assert!(s.probe_failed(t0 + Duration::from_secs(4)));
        assert!(!s.probe_failed(t0 + READY_BUDGET));
        assert_eq!(s.phase(), Phase::Idle);
    }

    #[test]
    fn resume_does_not_interrupt_a_connect_but_a_connect_wins() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(100);
        let mut s = Scheduler::default();
        s.event(Trigger::Connect, t0);
        s.resumed(t1);
        assert!(matches!(
            s.phase(),
            Phase::WaitReady { trigger: Trigger::Connect, event_at, .. } if event_at == t0
        ));
        s.applied(t0, t1);
        s.resumed(t1);
        assert!(matches!(
            s.phase(),
            Phase::WaitReady {
                trigger: Trigger::Resume,
                ..
            }
        ));
        s.event(Trigger::Connect, t1);
        s.removed();
        assert_eq!(s.phase(), Phase::Idle);
    }
}

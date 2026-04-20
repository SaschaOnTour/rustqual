//! Contract tests for the `Reporter` port.

use crate::ports::{ReportError, ReportPayload, Reporter};
use std::sync::Mutex;

struct RecordingReporter {
    calls: Mutex<Vec<String>>,
}

impl Reporter for RecordingReporter {
    fn emit(&self, payload: &ReportPayload) -> Result<(), ReportError> {
        self.calls.lock().unwrap().push(payload.placeholder.clone());
        Ok(())
    }
}

#[test]
fn port_is_object_safe() {
    let _boxed: Box<dyn Reporter> = Box::new(RecordingReporter {
        calls: Mutex::new(vec![]),
    });
}

#[test]
fn port_requires_send_and_sync() {
    let _: Box<dyn Send + Sync> = Box::new(RecordingReporter {
        calls: Mutex::new(vec![]),
    });
}

#[test]
fn reporter_receives_emit_call() {
    let reporter = RecordingReporter {
        calls: Mutex::new(vec![]),
    };
    reporter
        .emit(&ReportPayload {
            placeholder: "hello".into(),
        })
        .unwrap();
    let calls = reporter.calls.lock().unwrap();
    assert_eq!(calls.as_slice(), &["hello".to_string()]);
}

#[test]
fn report_error_variants_carry_diagnostic_information() {
    let e = ReportError::Io("broken pipe".into());
    assert!(e.to_string().contains("broken pipe"));

    let e = ReportError::Encoding("invalid utf-8".into());
    assert!(e.to_string().contains("invalid utf-8"));
}

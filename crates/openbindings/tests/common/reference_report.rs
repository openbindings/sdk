#![allow(dead_code)]
use openbindings::{Reference, ReferenceRefusal, ReferenceReport};

pub fn complete(report: &ReferenceReport) -> &[Reference] {
    let ReferenceReport::Complete { references } = report else {
        panic!("expected complete reference inspection")
    };
    references
}

pub fn incomplete(report: &ReferenceReport) -> (&[Reference], &ReferenceRefusal) {
    let ReferenceReport::Incomplete {
        retained,
        limitation,
    } = report
    else {
        panic!("expected incomplete reference inspection")
    };
    (retained, limitation)
}

//! Application policy around retained partial results and typed refusals.
use openbindings::{JsonValue, Source};
use openbindings_source_inspection::CancellationToken;
use openbindings_source_inspection::{InspectionError, InspectionErrorKind, Inspector};
use source_inspection_consumer::{
    AuthoringFailure, AuthoringOutcome, CATALOG_KIND, CatalogInspector, author_selected,
    author_target,
};

fn explain_refusal(error: &InspectionError) {
    match error.kind() {
        InspectionErrorKind::Cancelled => println!("Cancelled; wait for another user action."),
        InspectionErrorKind::UnsupportedKind => {
            println!("Choose a provider supporting this exact kind.")
        }
        InspectionErrorKind::UnreadableSource => {
            println!("Check the source and its access prerequisites.")
        }
        _ => println!("Inspection needs attention."),
    }
    println!("{}: {}", error.code(), error.message());
}

fn present(outcome: AuthoringOutcome) {
    match outcome {
        AuthoringOutcome::Ready(document) => println!("Ready: {} bytes", document.text().len()),
        AuthoringOutcome::Empty => println!("No admitted targets; there is nothing to select."),
        AuthoringOutcome::Partial(inspection) => {
            let limitation = inspection
                .limitation
                .as_ref()
                .expect("checked partial inventory");
            println!("{}: {}", limitation.code, limitation.message);
            println!(
                "{} known targets remain available for explicit selection.",
                inspection.targets.len()
            );
        }
        AuthoringOutcome::NeedsRepair { draft, validation } => {
            for finding in &validation.report().findings {
                println!(
                    "{} at {:?}: {}",
                    finding.code, finding.location, finding.message
                );
            }
            println!(
                "Retained {} draft operations for editing.",
                draft.operations.len()
            );
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = Source {
        kind: CATALOG_KIND.into(),
        content: Some(JsonValue::parse(
            r#"[
            {"id":"available","status":"admitted","binding":null},
            {"id":"missing","status":"unavailable"}
        ]"#,
        )?),
        ..Default::default()
    };
    match author_selected(&source).await {
        Ok(AuthoringOutcome::Partial(inspection)) => {
            // Present the evidence before an application/user explicitly accepts
            // choosing one known target. This never means the inventory is complete.
            present(AuthoringOutcome::Partial(inspection.clone()));
            let selected = inspection
                .targets
                .into_iter()
                .next()
                .expect("fixture has one known target");
            present(author_target(&source, selected)?);
        }
        Ok(outcome) => present(outcome),
        Err(AuthoringFailure::Inspection(error)) => explain_refusal(&error),
        Err(error) => return Err(error.into()),
    }

    let provider = CatalogInspector;
    let inspector = Inspector::new(&provider);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    explain_refusal(
        &inspector
            .inspect_with_control(&source, &cancelled)
            .await
            .unwrap_err(),
    );
    let unsupported = Source {
        kind: "urn:another-kind".into(),
        ..Default::default()
    };
    explain_refusal(&inspector.inspect(&unsupported).await.unwrap_err());
    let unreadable = Source {
        kind: CATALOG_KIND.into(),
        ..Default::default()
    };
    explain_refusal(&inspector.inspect(&unreadable).await.unwrap_err());
    Ok(())
}

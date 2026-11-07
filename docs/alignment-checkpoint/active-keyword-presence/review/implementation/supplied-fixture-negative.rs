
#[test]fn review_supplied_repeated_shorthand_fixture_is_not_the_contained_job(){
 use openbindings::{Document,ExternalSchema,ExternalSchemas,JsonValue,PreparationLimits};
 use crate::DefaultEvaluator;
 use std::sync::atomic::AtomicBool;
 let properties=(0..380).map(|i|(format!("p{i}"),serde_json::json!({"pattern":format!("{}z0",r"\S".repeat(5000))}))).collect::<serde_json::Map<_,_>>();
 let root="https://mask.test/root";
 let proof=Document::parse(serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":root}}}}).to_string()).unwrap().validate().unwrap().conformant().unwrap();
 let extras=ExternalSchemas::new(vec![ExternalSchema{uri:root.into(),document:JsonValue::parse(serde_json::json!({"properties":properties}).to_string()).unwrap()}]).unwrap();
 let result=proof.operation("op").unwrap().input().unwrap().prepare(&DefaultEvaluator::default(),&extras,&PreparationLimits::default());
 match result{Ok(_)=>panic!("supplied variant unexpectedly ready"),Err(f)=>println!("SUPPLIED_NEGATIVE {:?}",f)}
 let _=AtomicBool::new(false);
}

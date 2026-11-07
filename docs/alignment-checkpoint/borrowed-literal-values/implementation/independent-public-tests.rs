use super::*;
use std::sync::atomic::{AtomicBool,AtomicUsize,Ordering::Relaxed};
use serde_json::{json,Value as J};
fn plan(schema:&str, extra:&[(&str,&str)]) -> Plan {
 let proof=Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://review.test/root"}}}}"#).unwrap().validate().unwrap().conformant().unwrap();
 let mut rs=vec![ExternalSchema{uri:"https://review.test/root".into(),document:JsonValue::parse(schema).unwrap()}];rs.extend(extra.iter().map(|(u,s)|ExternalSchema{uri:(*u).into(),document:JsonValue::parse(s).unwrap()}));
 let ext=ExternalSchemas::new(rs).unwrap();let c=AtomicBool::new(false);let p=PreparationLimits::default();let env=proof.operation("op").unwrap().input().unwrap().environment(&ext,&p,&c).unwrap();
 openbindings_internal_schema::preparation::Attempt::run(p.max_sdk_work_units,p.max_sdk_reservation_bytes,&c,|a|Plan::new(env,Limits::default(),p,a)).unwrap().unwrap()
}
fn observed(o:ValueOutcome)->String {match o {ValueOutcome::Satisfies=>"S".into(),ValueOutcome::Fails{..}=>"F".into(),ValueOutcome::Undefined{..}=>"U".into(),ValueOutcome::NoVerdict{detail}=>format!("N:{:?}",detail.reason)}}
fn check(schema:&str,value:&str,expected:&str){let p=plan(schema,&[]);let v=JsonValue::parse(value).unwrap();for d in [Detail::Outcome,Detail::Explain]{assert_eq!(observed(p.evaluate(&v,d,&AtomicBool::new(false))),expected,"schema={schema}, value={value}");}}

#[test]fn e10_d01_d04_c02_c03_public_domains(){for(s,v,e)in[(r#"{"const":{"a":[1,2]}}"#,r#"{"a":[1.0,2.0]}"#,"S"),(r#"{"enum":[{"a":[2]},{"a":[1]},{"a":[3]}]}"#,r#"{"a":[1.0]}"#,"S"),(r#"{"enum":[{},[]]}"#,"false","F"),(r#"{"uniqueItems":true}"#,r#"[{"a":1,"a":1}]"#,"N:UnsupportedCapability")]{check(s,v,e);}let s=r##"{"$defs":{"bad":{"$ref":"%"}},"anyOf":[{"prefixItems":[{"$ref":"#/$defs/bad"},true]},{"prefixItems":[true,{"not":{"$ref":"#/$defs/bad"}}]}]}"##;check(s,"[1,1.0]","S");check(s,"[1,2]","U");check(r##"{"$defs":{"A":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/A"}}]}},"$ref":"#/$defs/A"}"##,"[[1],[1]]","S");let p=plan(r#"{"const":{"a":[1,2]},"uniqueItems":true}"#,&[]);std::thread::scope(|scope|{for _ in 0..4{scope.spawn(||{let v=JsonValue::parse(r#"{"a":[1.0,2]}"#).unwrap();for _ in 0..30{assert_eq!(observed(p.evaluate(&v,Detail::Outcome,&AtomicBool::new(false))),"S");}});}});for s in[r#"{"const":{"x":1,"x":1}}"#,r#"{"enum":[{"x":1,"x":1},{}]}"#]{assert!(ExternalSchemas::new([ExternalSchema{uri:"https://review.test/dup".into(),document:JsonValue::parse(s).unwrap()}]).is_err());}}

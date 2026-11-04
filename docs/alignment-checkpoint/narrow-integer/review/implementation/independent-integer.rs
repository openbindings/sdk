use super::*;
use openbindings_internal_schema::Control as _;
use serde_json::{json,Value};
use std::sync::atomic::{AtomicBool,AtomicUsize,Ordering::Relaxed};
struct Count{calls:AtomicUsize,at:AtomicUsize}
impl WorkControl for Count{fn is_cancelled(&self)->bool{let n=self.calls.fetch_add(1,Relaxed);super::review_inline_trace((2,n));n>=self.at.load(Relaxed)}}
fn run(value:JsonRef<'_>,new:bool,bytes:usize,work:usize,spent:usize,stop:usize)->Value{
 let c=Count{calls:AtomicUsize::new(0),at:AtomicUsize::new(usize::MAX)};let mut g=Graph::new(bytes,Some(work),64,&c);
 // Pre-spending is successful; prior ignored exhaustion is tested with real Attempt separately.
 if spent!=0{g.work(spent).unwrap();g.charge(spent).unwrap();}
 c.calls.store(0,Relaxed);c.at.store(stop,Relaxed);super::review_inline_trace_start();
 let answer=if new{value_predicates::integer(value,&mut g)}else{value_predicates::number(value,&mut g).map(|d|d.is_integer())};
 let trace=super::review_inline_trace_take();
 let answer=match answer{Ok(v)=>json!({"integer":v}),Err(e)=>json!({"reason":format!("{:?}",e.reason),"code":e.code,"message":e.message})};
 json!({"answer":answer,"remaining_work":g.remaining().work,"remaining_bytes":g.remaining().bytes,"checks":c.calls.load(Relaxed),"trace":trace})
}
#[test]fn frozen52_answers_exact_charges_and_all_stops(){
 let data:Value=serde_json::from_str(include_str!("integer-controls.json")).unwrap();let rows=data["token_rows"].as_array().unwrap();assert_eq!(rows.len(),52);let mut comparisons=0;
 for row in rows{let value=JsonValue::parse(row["token"].as_str().unwrap()).unwrap();let w=row["expected_graph_work"].as_u64().unwrap()as usize;let m=row["expected_graph_reservation"].as_u64().unwrap()as usize;
  for spent in[0,7]{for (label,bytes,work)in[("funded",1<<24,1<<24),("exact",m+spent,w+spent),("work-below",m+spent,w+spent-1),("memory-below",m.saturating_sub(1)+spent,w+spent),("zero-extra-memory",spent,w+spent)]{
   let old=run(value.view(),false,bytes,work,spent,usize::MAX);let new=run(value.view(),true,bytes,work,spent,usize::MAX);assert_eq!(old,new,"{} {label}/{spent}",row["id"]);comparisons+=1;
   if label=="funded"||label=="exact"{assert_eq!(new["answer"]["integer"],row["integer"]);assert_eq!(new["remaining_work"],json!(work-spent-w));assert_eq!(new["remaining_bytes"],json!(bytes-spent-m));assert_eq!(new["checks"],1);}
   let n=new["checks"].as_u64().unwrap()as usize;let mut stopped=Vec::new();for stop in 0..n{let a=run(value.view(),false,bytes,work,spent,stop);let b=run(value.view(),true,bytes,work,spent,stop);assert_eq!(a,b);assert_eq!(b["answer"]["reason"],"Cancelled");stopped.push(b);comparisons+=1;}
   assert_eq!(run(value.view(),true,bytes,work,spent,usize::MAX),new);
   println!("INTEGER_TRACE {}",json!({"id":row["id"],"budget":label,"spent":spent,"record":new,"stops":stopped}));
  }}
 }
 println!("INTEGER_COMPARISONS {comparisons}");
}
#[test]fn prior_shared_attempt_causes_and_cancel_are_not_bypassed(){
 use openbindings_internal_schema::preparation::Attempt;
 for token in["0","1.5","9223372036854775808"]{let value=JsonValue::parse(token).unwrap();for memory in[false,true]{let one=|new|{let mut observed=Value::Null;let result=Attempt::run(if memory{usize::MAX}else{1},if memory{1}else{usize::MAX},&||false,|a|{
  if memory{let _=a.charge_allocation(2);}else{let _=a.charge_work(2);}
  let c=AtomicBool::new(false);let mut g=Graph::with_preparation(usize::MAX,None,64,&c,Some(a));let out=if new{value_predicates::integer(value.view(),&mut g)}else{value_predicates::number(value.view(),&mut g).map(|d|d.is_integer())};
  let e=out.unwrap_err();observed=json!({"reason":format!("{:?}",e.reason),"code":e.code,"message":e.message});
 });let failure=result.unwrap_err();(observed,format!("{:?}",failure.reason),failure.code.into_owned())};assert_eq!(one(false),one(true));}}
}
fn public(schema:Value,value:&str,custom:bool)->ValueOutcome{
 let meta="https://integer.test/meta";let mut schema=schema;if custom{schema.as_object_mut().unwrap().insert("$schema".into(),json!(meta));}
 public_raw(JsonValue::parse(schema.to_string()).unwrap(),value,custom)
}
fn public_raw(schema:JsonValue,value:&str,custom:bool)->ValueOutcome{
 let meta="https://integer.test/meta";let root="https://integer.test/root";let proof=Document::parse(json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":root}}}}).to_string()).unwrap().validate().unwrap().conformant().unwrap();
 let mut resources=vec![ExternalSchema{uri:root.into(),document:schema}];if custom{resources.push(ExternalSchema{uri:meta.into(),document:JsonValue::parse(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/validation":true},"type":"object"}"#).unwrap()});}
 let p=proof.operation("op").unwrap().input().unwrap().prepare(&crate::DefaultEvaluator::default(),&ExternalSchemas::new(resources).unwrap(),&PreparationLimits::default()).unwrap();p.explain(&JsonValue::parse(value).unwrap())
}
#[test]fn kind_type_name_and_union_eagerness_boundaries(){
 for text in["null","false","true",r#""1""#,"[]","{}"]{let o=public(json!({"type":"integer"}),text,false);assert!(matches!(o,ValueOutcome::Fails{..}));}
 for(schema,text)in[(json!({"type":["integer","string"]}),r#""x""#),(json!({"type":["string","integer"]}),"1")]{assert!(matches!(public(schema,text,false),ValueOutcome::Satisfies));}
 for types in[json!(null),json!(17),json!("unknown"),json!([["integer"]]),json!(["integer",17]),json!([17,"integer"]) ]{let o=public(json!({"type":types}),"1",true);assert!(matches!(o,ValueOutcome::NoVerdict{detail}if detail.reason==NoVerdictReason::UnsupportedCapability));}
 assert!(matches!(public(json!({"type":[]}),"1",true),ValueOutcome::Fails{..}));
 let lone=public_raw(JsonValue::parse(r#"{"$schema":"https://integer.test/meta","type":"\ud800"}"#).unwrap(),"1",true);
 assert!(matches!(lone,ValueOutcome::NoVerdict{detail}if detail.reason==NoVerdictReason::UnsupportedCapability));
 for text in["+1","01","1.","NaN","Infinity","--1"]{assert!(JsonValue::parse(text).is_err());}
}
#[test]fn literal_actual_allocations_and_cleanup_match_old_path(){
 let data:Value=serde_json::from_str(include_str!("integer-controls.json")).unwrap();for row in data["token_rows"].as_array().unwrap(){let value=JsonValue::parse(row["token"].as_str().unwrap()).unwrap();let mut all=Vec::new();for new in[false,true]{let c=AtomicBool::new(false);let snap=crate::review_allocations::start();for _ in 0..16{let mut g=Graph::new(1<<24,Some(1<<24),64,&c);let out=if new{value_predicates::integer(value.view(),&mut g)}else{value_predicates::number(value.view(),&mut g).map(|d|d.is_integer())};std::hint::black_box(out.unwrap());}let m=crate::review_allocations::finish(snap,16);assert_eq!(m["ending_live_bytes"],m["baseline_live_bytes"]);all.push(m);}
 for key in["allocations_per_op","requested_bytes_per_op","incremental_peak_bytes"]{assert_eq!(all[0][key],all[1][key],"{} {key}",row["id"]);if row["path"]=="i64"{assert_eq!(all[1][key].as_f64().unwrap(),0.0);}}
 println!("INTEGER_ALLOCATION {}",json!({"id":row["id"],"old":all[0],"new":all[1]}));
 }
}

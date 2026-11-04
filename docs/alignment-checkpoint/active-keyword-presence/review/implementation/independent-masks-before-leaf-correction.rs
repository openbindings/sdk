use super::*;
use serde_json::{json,Value as J};
use std::sync::atomic::{AtomicBool,AtomicUsize,Ordering::Relaxed};
const ROOT:&str="https://mask.test/root";
const META:&str="https://mask.test/meta";
const PERMISSIVE:&str=r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/applicator":true,"https://json-schema.org/draft/2020-12/vocab/validation":true,"https://json-schema.org/draft/2020-12/vocab/unevaluated":true},"type":"object"}"#;
const KEYS:&[&str]=&["type","const","enum","minimum","maximum","exclusiveMinimum","exclusiveMaximum","multipleOf","minLength","maxLength","minItems","maxItems","minProperties","maxProperties","uniqueItems","required","dependentRequired","dependencies","format","pattern","$ref","$dynamicRef","$recursiveRef","allOf","anyOf","oneOf","not","if","then","else","dependentSchemas","properties","patternProperties","additionalProperties","propertyNames","unevaluatedProperties","items","prefixItems","additionalItems","contains","minContains","maxContains","unevaluatedItems"];
fn env(schema:&str, extras:&[(&str,&str)])->SchemaEnvironment{
 let proof=Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{ROOT}"}}}}}}}}"#)).unwrap().validate().unwrap().conformant().unwrap();
 let mut resources=vec![ExternalSchema{uri:ROOT.into(),document:JsonValue::parse(schema).unwrap()}];resources.extend(extras.iter().map(|(u,s)|ExternalSchema{uri:(*u).into(),document:JsonValue::parse(s).unwrap()}));
 proof.operation("op").unwrap().input().unwrap().environment(&ExternalSchemas::new(resources).unwrap(),&PreparationLimits::default(),&AtomicBool::new(false)).unwrap()
}
fn plan(schema:&str,extras:&[(&str,&str)])->Plan{
 let e=env(schema,extras);let p=PreparationLimits::default();openbindings_internal_schema::preparation::Attempt::run(p.max_sdk_work_units,p.max_sdk_reservation_bytes,&||false,|a|Plan::new(e,Limits::default(),p,a)).unwrap().unwrap()
}
fn root(p:&Plan,s:&str)->usize{*p.fields.keys().find(|&&n|p.environment.space().node(n).value.text()==s).unwrap()}
fn status(o:&ValueOutcome)->String{match o{ValueOutcome::Satisfies=>"S".into(),ValueOutcome::Fails{..}=>"F".into(),ValueOutcome::Undefined{..}=>"U".into(),ValueOutcome::NoVerdict{detail}=>format!("N:{:?}",detail.reason)}}
fn check(s:&str,v:&str,want:&str){let p=plan(s,&[]);let value=JsonValue::parse(v).unwrap();for d in[Detail::Outcome,Detail::Explain]{assert_eq!(status(&p.evaluate(&value,d,&AtomicBool::new(false))),want,"{s} {v}");}}
fn signature(f:readings::Fields)->Vec<bool>{KEYS.iter().map(|k|f.may_read(k)).chain([f.logical(),f.object(),f.array(),f.is_leaf()]).collect()}
fn original_get(profile:Profile,v:JsonRef<'_>,k:&str)->Option<String>{profile.get(v,k).map(|v|v.text().to_owned())}

#[test]fn md01_md04_all_mapped_getters_profiles_and_exact_names(){
 let core=JsonValue::parse(r#"{"https://json-schema.org/draft/2020-12/vocab/core":true}"#).unwrap();
 let optional=JsonValue::parse(r#"{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/validation":false,"https://json-schema.org/draft/2020-12/vocab/applicator":false}"#).unwrap();
 let profiles=[Profile::standard(Dialect::Draft7),Profile::standard(Dialect::Draft201909),Profile::standard(Dialect::Draft202012),Profile::from_vocabulary(Some(core.view())).unwrap(),Profile::from_vocabulary(Some(optional.view())).unwrap()];
 let mut rows=0;
 for profile in profiles{for key in KEYS{for with_ref in[false,true]{let mut map=serde_json::Map::new();map.insert((*key).into(),json!(17));if with_ref{map.insert("$ref".into(),json!(false));}let value=JsonValue::parse(json!(map).to_string()).unwrap();
  let mut fields=readings::Fields::EMPTY;for member in value.view().members().unwrap(){let k=member.name.as_str().unwrap();if profile.get(value.view(),k).is_some(){fields.include(k);}}
  let interpreted=Interpreted{value:value.view(),profile,fields};for query in KEYS.iter().copied().chain(["$defs","contentSchema","$recursiveAnchor","future"]){assert_eq!(interpreted.get(query).map(|v|v.text().to_owned()),original_get(profile,value.view(),query));}rows+=1;
 }}}
 for s in[r#"{"\u0074ype":"integer","future":false,"\ud800":false}"#,r#"{"$ref":false,"\u0074ype":"integer"}"#]{let v=JsonValue::parse(s).unwrap();for profile in profiles{let mut f=readings::Fields::EMPTY;for m in v.view().members().unwrap(){if let Some(k)=m.name.as_str(){if profile.get(v.view(),k).is_some(){f.include(k);}}}let view=Interpreted{value:v.view(),profile,fields:f};for k in KEYS.iter().copied().chain(["future"]){assert_eq!(view.get(k).map(|v|v.text().to_owned()),original_get(profile,v.view(),k));}}}
 println!("MD01 getter rows={rows} mapped={}",KEYS.len());assert_eq!(KEYS.len(),43);
}
#[test]fn md02_md05_complete_scan_order_and_independent_leaf(){
 for (left,right) in[(r#"{"then":false,"if":true}"#,r#"{"if":true,"then":false}"#),(r#"{"$schema":"http://json-schema.org/draft-07/schema#","additionalItems":false,"items":[true]}"#,r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":[true],"additionalItems":false}"#),(r##"{"$schema":"http://json-schema.org/draft-07/schema#","type":"integer","$ref":"#/$defs/nope"}"##,r##"{"$ref":"#/$defs/nope","type":"integer","$schema":"http://json-schema.org/draft-07/schema#"}"##)]{let a=plan(left,&[]);let b=plan(right,&[]);assert_eq!(signature(a.fields(root(&a,left))),signature(b.fields(root(&b,right))));}
 for s in[r#"{"$defs":{"x":true}}"#,r#"{"definitions":{"x":true}}"#,r#"{"contentSchema":true}"#,r#"{"then":false}"#]{let p=plan(s,&[]);let f=p.fields(root(&p,s));assert!(!f.is_leaf(),"{s}");}
 for (k,want)in[("anyOf","F"),("oneOf","F"),("allOf","S")]{let s=json!({"$schema":META,k:[]}).to_string();let p=plan(&s,&[(META,PERMISSIVE)]);let f=p.fields(root(&p,&s));assert!(!f.is_leaf());assert!(f.may_read(k));for v in["null","false","0",r#""x""#,"[]","{}"]{for d in[Detail::Outcome,Detail::Explain]{assert_eq!(status(&p.evaluate(&JsonValue::parse(v).unwrap(),d,&AtomicBool::new(false))),want);}}}
}
#[test]fn md03_original_missing_ineligible_unknown_and_boolean_paths(){
 let p=plan(r#"{"type":"integer"}"#,&[]);let f=p.fields(usize::MAX);assert!(!f.is_leaf());assert!(f.logical()&&f.object()&&f.array());assert!(KEYS.iter().all(|k|f.may_read(k)));
 for(s,v,want)in[("true","null","S"),("false","null","F"),("{}","[]","S")]{check(s,v,want);}
 let p=plan(r#"{"$schema":"https://unknown.test/meta","type":"integer"}"#,&[]);assert!(!p.fields.keys().any(|&n|p.environment.space().node(n).value.get("$schema").is_some()));let o=p.evaluate(&JsonValue::parse("1").unwrap(),Detail::Outcome,&AtomicBool::new(false));assert_eq!(status(&o),"N:UnsupportedCapability");
 for k in["allOf","anyOf"]{let s=json!({k:[k=="anyOf",{"$schema":"https://unknown.test/meta","type":"integer"}]}).to_string();let p=plan(&s,&[]);let o=p.evaluate(&JsonValue::parse("1").unwrap(),Detail::Outcome,&AtomicBool::new(false));assert_eq!(status(&o),if k=="allOf"{"F"}else{"S"});}
}
#[test]fn md07_md08_md09_every_phase_trigger(){
 for s in[r#"{"allOf":[false]}"#,r#"{"anyOf":[false]}"#,r#"{"oneOf":[true,true]}"#,r#"{"not":true}"#,r#"{"if":true,"then":false}"#,r#"{"if":false,"else":false}"#]{check(s,"1","F");}
 for s in[r#"{"properties":{"x":false}}"#,r#"{"patternProperties":{".":false}}"#,r#"{"additionalProperties":false}"#,r#"{"propertyNames":false}"#,r#"{"dependentSchemas":{"x":false}}"#,r#"{"$schema":"http://json-schema.org/draft-07/schema#","dependencies":{"x":false}}"#,r#"{"unevaluatedProperties":false}"#]{check(s,r#"{"x":1}"#,"F");}
 for(s,v)in[(r#"{"items":false}"#,"[1]"),(r#"{"prefixItems":[false]}"#,"[1]"),(r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":[true],"additionalItems":false}"#,"[1,2]"),(r#"{"contains":false}"#,"[1]"),(r#"{"contains":true,"minContains":2}"#,"[1]"),(r#"{"contains":true,"maxContains":0}"#,"[1]"),(r#"{"unevaluatedItems":false}"#,"[1]")]{check(s,v,"F");}
}
#[test]fn md10_md12_state_preservation_and_alias_target_refresh(){
 for(s,v,want)in[(r#"{"allOf":[{"allOf":[{"properties":{"x":true}}]}],"unevaluatedProperties":false}"#,r#"{"x":1}"#,"S"),(r#"{"allOf":[{"allOf":[{"prefixItems":[true]}]}],"unevaluatedItems":false}"#,"[1]","S"),(r##"{"$defs":{"a":{"allOf":[{"properties":{"x":true}}]}},"allOf":[{"$ref":"#/$defs/a"}],"unevaluatedProperties":false}"##,r#"{"x":1,"y":2}"#,"F"),(r##"{"$defs":{"a":{"items":{"type":"integer"}}},"$ref":"#/$defs/a"}"##,"[1,2]","S"),(r##"{"$defs":{"a":{"properties":{"x":false}}},"$ref":"#/$defs/a"}"##,r#"{"x":1}"#,"F")]{check(s,v,want);}
}
struct Stop {calls:AtomicUsize,at:usize}
impl openbindings_internal_schema::Control for Stop{fn is_cancelled(&self)->bool{self.calls.fetch_add(1,Relaxed)>=self.at}}
#[test]fn md15_all_publication_stops_and_real_attempt_boundaries(){
 let s=r#"{"allOf":[{"properties":{"x":{"type":"integer"}}},{"if":true,"then":{"required":["x"]}}],"unevaluatedProperties":false}"#;
 let e=env(s,&[]);let p=PreparationLimits::default();Plan::new(e.clone(),Limits::default(),p,&||false).unwrap();
 let c=Stop{calls:AtomicUsize::new(0),at:usize::MAX};let ready=Plan::new(e.clone(),Limits::default(),p,&c).unwrap();let n=c.calls.load(Relaxed);let count=ready.fields.len();drop(ready);
 for at in 0..n{let c=Stop{calls:AtomicUsize::new(0),at};assert!(Plan::new(e.clone(),Limits::default(),p,&c).is_err(),"stop {at}");assert_eq!(Plan::new(e.clone(),Limits::default(),p,&||false).unwrap().fields.len(),count);}
 for memory in[false,true]{let mut lo=0;let mut hi=if memory{1<<20}else{1<<24};let run=|cap|openbindings_internal_schema::preparation::Attempt::run(if memory{usize::MAX}else{cap},if memory{cap}else{usize::MAX},&||false,|a|Plan::new(e.clone(),Limits::default(),p,a)).is_ok_and(|r|r.is_ok());assert!(run(hi));while lo<hi{let mid=lo+(hi-lo)/2;if run(mid){hi=mid}else{lo=mid+1}}assert!(run(lo));assert!(!run(lo-1));assert!(run(lo));println!("MD15 memory={memory} minimum={lo}");}
 println!("MD15 publication checkpoints={n}");
}
#[test]fn md16_no_dense_unused_inventory_and_actual_map_reservation(){
 let s=r#"{"type":"integer"}"#;let unused=json!({"$defs":(0..5000).map(|i|(format!("n{i}"),json!({"type":"integer"}))).collect::<serde_json::Map<_,_>>()}).to_string();let a=plan(s,&[]);let b=plan(s,&[("https://unused.test/schema",&unused)]);assert_eq!(a.fields.len(),b.fields.len());assert_eq!(size_of::<readings::Fields>(),8);
 for n in[1,2,4,8,15,29,57,113,225,449,897,1793,3585]{let snap=crate::review_allocations::start();let mut map=HashMap::new();for i in 0..n{map.insert(i,readings::Fields::EMPTY);}let cap=map.capacity();drop(map);let m=crate::review_allocations::finish(snap,1);assert!(m["requested_bytes_per_op"].as_f64().unwrap()<=(136*n)as f64);assert_eq!(m["ending_live_bytes"],m["baseline_live_bytes"]);println!("MD16 map n={n} capacity={cap} metrics={m}");}
}
#[test]fn md19_absent_getter_never_reaches_raw_lookup(){
 let v=JsonValue::parse(r#"{"type":"integer","future":17}"#).unwrap();let profile=Profile::standard(Dialect::Draft202012);let mut f=readings::Fields::EMPTY;f.include("type");
 super::review_raw_reads_reset();let schema=Interpreted{value:v.view(),profile,fields:f};for key in KEYS{let _=schema.get(key);}assert_eq!(super::review_raw_reads(),1);assert_eq!(schema.get("future").unwrap().number_text(),Some("17"));assert_eq!(super::review_raw_reads(),2);
 super::review_raw_reads_reset();let schema=Interpreted{value:v.view(),profile,fields:readings::Fields::ORIGINAL};for key in KEYS{let _=schema.get(key);}assert_eq!(super::review_raw_reads(),43);
}
#[test]fn md17_large_array_object_defaults(){
 let valid=format!("[{}1]","1,".repeat(19999));let bad=format!("[{},\"late\"]",&valid[1..valid.len()-1]);check(r#"{"items":{"type":"integer"}}"#,&valid,"S");check(r#"{"items":{"type":"integer"}}"#,&bad,"F");
 let object=json!((0..50000).map(|i|(format!("k{i}"),json!(i))).collect::<serde_json::Map<_,_>>()).to_string();check(r#"{"additionalProperties":{"type":"integer"}}"#,&object,"S");
 let absent=r#"{"allOf":[true]}"#;check(absent,&object,"S");check(absent,&valid,"S");
}

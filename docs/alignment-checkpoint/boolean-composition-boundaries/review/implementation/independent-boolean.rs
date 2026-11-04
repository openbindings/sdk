use super::*;
use openbindings_internal_schema::{Control as _, preparation::Attempt};
use serde_json::json;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool,AtomicUsize,Ordering::Relaxed};
thread_local! {static LOOP:Cell<(usize,usize,usize)>=const{Cell::new((usize::MAX,0,usize::MAX))};}
pub(super) fn loop_visit(id:usize){LOOP.with(|l|{let(t,n,stop)=l.get();if t==id{l.set((t,n+1,stop));}})}
fn loop_set(id:usize,stop:usize){LOOP.with(|l|l.set((id,0,stop)));}
fn loop_count()->usize{LOOP.with(|l|l.get().1)}
struct LoopControl;
impl WorkControl for LoopControl{fn is_cancelled(&self)->bool{LOOP.with(|l|{let(_,n,stop)=l.get();n>stop})}}
struct Counter{calls:AtomicUsize,cancelled:AtomicBool}
impl Counter{fn healthy()->Self{Self{calls:AtomicUsize::new(0),cancelled:AtomicBool::new(false)}}}
impl WorkControl for Counter{fn is_cancelled(&self)->bool{self.calls.fetch_add(1,Relaxed);self.cancelled.load(Relaxed)}}
fn graph<'a>(c:&'a dyn WorkControl)->Graph<'a>{Graph::new(1<<24,Some(1<<24),64,c)}
#[test]fn algebra_terminal_results_are_pure_for_all_reused_formula_kinds(){
 let c=Counter::healthy();let mut g=graph(&c);let(_,d)=g.variable(decision::Class::Undefined).unwrap();let(_,e)=g.variable(decision::Class::Inability).unwrap();let nd=g.not(d).unwrap();
 for x in[FALSE,TRUE,d,e,nd]{for(a,b,and,or)in[(FALSE,x,FALSE,x),(x,FALSE,FALSE,x),(TRUE,x,x,TRUE),(x,TRUE,x,TRUE),(x,x,x,x)]{
  let before=g.review_state();c.calls.store(0,Relaxed);c.cancelled.store(true,Relaxed);
  assert_eq!(g.and(a,b).unwrap(),and);assert_eq!(g.or(a,b).unwrap(),or);assert_eq!(g.review_state(),before);assert_eq!(c.calls.load(Relaxed),0);
 }}
 assert!(g.and(d,e).is_err());assert!(g.or(d,e).is_err());assert!(g.not(TRUE).is_err());
}
#[test]fn nontrivial_fallback_and_cached_recursion_keep_actual_work_and_control(){
 for and in[false,true]{for repeat in[1,2]{let run=|wrapper|{let c=Counter::healthy();let mut g=graph(&c);let(_,a)=g.variable(decision::Class::Undefined).unwrap();let(_,b)=g.variable(decision::Class::Inability).unwrap();c.calls.store(0,Relaxed);let mut value=0;
 for _ in 0..repeat{value=if wrapper{if and{g.and(a,b)}else{g.or(a,b)}}else{g.review_apply(and,a,b)}.unwrap();}
 (value,g.review_state(),c.calls.load(Relaxed))};assert_eq!(run(true),run(false));}}
}
#[test]fn shared_attempt_zero_prior_nested_overflow_and_cancellation(){
 let normal=Counter::healthy();let healthy=AtomicBool::new(false);
 Attempt::run(0,0,&healthy,|a|{let mut g=Graph::with_preparation(0,Some(0),0,&normal,Some(a));for _ in 0..16{assert_eq!(g.and(TRUE,FALSE).unwrap(),FALSE);assert_eq!(g.or(FALSE,TRUE).unwrap(),TRUE);}assert_eq!(normal.calls.load(Relaxed),0);}).unwrap();
 for mode in["work","allocation","nested-work","nested-allocation","overflow-work","overflow-allocation","cancel"]{
  let flag=AtomicBool::new(false);let result=Attempt::run(usize::MAX,usize::MAX,&flag,|a|{
   match mode{
    "work"=>{a.charge_work(usize::MAX).unwrap();assert!(a.charge_work(1).is_err());},
    "allocation"=>{let _=Attempt::run(usize::MAX,0,a,|b|b.charge_allocation(1));},
    "nested-work"=>{let _=Attempt::run(0,usize::MAX,a,|b|b.charge_work(1));},
    "nested-allocation"=>{let _=Attempt::run(usize::MAX,0,a,|b|b.charge_allocation(1));},
    "overflow-work"=>{a.charge_work(usize::MAX).unwrap();assert!(a.charge_work(usize::MAX).is_err());},
    "overflow-allocation"=>{a.charge_allocation(usize::MAX).unwrap();assert!(a.charge_allocation(1).is_err());},
    "cancel"=>flag.store(true,Relaxed),_=>unreachable!()}
   let mut g=Graph::with_preparation(usize::MAX,None,64,&normal,Some(a));
   for (a,b)in[(FALSE,TRUE),(TRUE,FALSE),(TRUE,TRUE),(FALSE,FALSE),(2,2),(TRUE,2),(2,FALSE)]{
    let x=g.and(a,b).unwrap_err();let y=g.or(a,b).unwrap_err();assert_eq!(x.code,y.code);assert_eq!(x.reason,y.reason);
    assert_eq!(x.reason,if mode=="cancel"{NoVerdictReason::Cancelled}else{NoVerdictReason::LimitExceeded});
    if mode.contains("allocation"){assert_eq!(x.code,"preparation-sdk-reservation-bytes");}else if mode!="cancel"{assert_eq!(x.code,"preparation-sdk-work-units");}
   }
  });assert!(result.is_err(),"{mode}");println!("SHARED_CASE {mode}");
 }
}
fn plan()->Plan{
 let proof=Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":true}}}"#).unwrap().validate().unwrap().conformant().unwrap();let s=proof.operation("op").unwrap().input().unwrap();let p=PreparationLimits::default();let c=AtomicBool::new(false);let e=s.environment(&ExternalSchemas::default(),&p,&c).unwrap();Attempt::run(p.max_sdk_work_units,p.max_sdk_reservation_bytes,&c,|a|Plan::new(e,Limits::default(),p,a)).unwrap().unwrap()
}
fn run<'a>(p:&'a Plan,c:&'a dyn WorkControl,work:usize,bytes:usize)->Run<'a>{Run{plan:p,record_witnesses:false,graph:Graph::new(bytes,Some(work),64,c),atoms:Vec::new(),values:values::Values::default(),effects:HashMap::new(),active:storage::Entries::default(),alias_path:storage::Entries::default(),empty_context:Arc::new(Vec::new()),opaque_cycles:HashSet::new(),pending_cycles:HashSet::new(),cuts:HashSet::new(),provisional:HashSet::new(),memo:HashMap::new(),facts:HashMap::new(),pending_facts:HashMap::new(),supports:HashMap::new(),support_memo_count:0,patterns:storage::Entries::default()}}
fn loop_run(p:&Plan,id:usize,width:usize,stop:usize,work:usize,bytes:usize)->(String,usize,usize,usize){
 loop_set(usize::MAX,usize::MAX);let c=LoopControl;let mut r=run(p,&c,work,bytes);let(_,d)=r.graph.variable(decision::Class::Undefined).unwrap();let mut state=State::truth(TRUE);
 for n in 0..width{let f=[FALSE,TRUE,d][n%3];if id==0&&n%2==0||id==1{let name=if n==0{vec![0xd800]}else{format!("k{n}").encode_utf16().collect()};state.properties.insert(name,f);}else{state.items.insert(n,f);}}
 loop_set(id,stop);let value=JsonValue::parse("null").unwrap();
 let out=if id==0{execution::review_complete(&mut r,value.view(),state).map(|out|{assert_eq!(out.properties.len()+out.items.len(),width);})}else if id==3{
  let mut v=vec![FALSE;width];v[width-1]=TRUE;let old=v.clone();let mut matches=counts::Matches::review_seed(v);let result=matches.add(d,&mut r.graph);if result.is_err(){assert_eq!(matches.review_values(),old);}else{assert_eq!(matches.review_values().len(),width+1);}
  result
 }else{let mut target=State::truth(TRUE);r.annotations(&mut target,&state,TRUE).map(|()|{assert_eq!(target.properties,state.properties);assert_eq!(target.items,state.items);})};
 let visits=loop_count();loop_set(usize::MAX,usize::MAX);assert!(r.memo.is_empty());assert!(r.facts.is_empty());assert!(r.pending_facts.is_empty());
 let result=match out{Ok(())=>"ok".to_owned(),Err(e)=>format!("{:?}:{}",e.reason,e.code)};let(_,_,_,used,steps)=r.graph.review_state();(result,visits,steps,used)
}
#[test]fn every_new_progress_loop_has_exact_actual_iteration_stops_and_budgets(){
 let p=plan();for id in 0..4{for width in[1,64,4096]{let a=loop_run(&p,id,width,usize::MAX,usize::MAX,usize::MAX);assert_eq!(a.0,"ok");assert_eq!(a.1,width);assert_eq!(loop_run(&p,id,width,usize::MAX,a.2,a.3).0,"ok");assert_eq!(loop_run(&p,id,width,usize::MAX,a.2-1,a.3).0,"LimitExceeded:evaluation-work-limit");assert_eq!(loop_run(&p,id,width,usize::MAX,a.2,a.3-1).0,"LimitExceeded:semantic-analysis-memory-limit");
 if width<=64{for stop in 0..width{let b=loop_run(&p,id,width,stop,usize::MAX,usize::MAX);assert_eq!(b.0,"Cancelled:caller-cancelled","id {id}/{width}/{stop}");assert_eq!(b.1,stop+1);assert_eq!(loop_run(&p,id,width,usize::MAX,usize::MAX,usize::MAX),a);}}
 println!("LOOP_RESULT {}",json!({"loop":id,"width":width,"checks":a.1,"work":a.2,"bytes":a.3}));}}
}
#[test]fn annotation_merges_and_completion_keep_exact_guarded_truth_tables(){
 let p=plan();let c=AtomicBool::new(false);let value=JsonValue::parse("null").unwrap();let mut checked=0;
 for guard_index in 0..4{for source_index in 0..4{for old_index in 0..5{
  let mut r=run(&p,&c,usize::MAX,usize::MAX);let(_,d)=r.graph.variable(decision::Class::Undefined).unwrap();let(_,e)=r.graph.variable(decision::Class::Inability).unwrap();let fs=[FALSE,TRUE,d,e];let guard=fs[guard_index];let source=fs[source_index];let mut src=State::truth(TRUE);let mut target=State::truth(TRUE);src.properties.insert(vec![0xd800],source);src.items.insert(7,source);
  if old_index<4{target.properties.insert(vec![0xd800],fs[old_index]);target.items.insert(7,fs[old_index]);}
  r.annotations(&mut target,&src,guard).unwrap();
  for assignment in[[false,false],[false,true],[true,false],[true,true]]{let eval=|x|r.graph.review_evaluate(x,&assignment);let expected=(old_index<4&&eval(fs[old_index]))||(eval(guard)&&eval(source));assert_eq!(eval(target.properties[&vec![0xd800]]),expected);assert_eq!(eval(target.items[&7]),expected);checked+=2;}
  src.valid=guard;let complete=execution::review_complete(&mut r,value.view(),src).unwrap();for assignment in[[false,false],[false,true],[true,false],[true,true]]{let eval=|x|r.graph.review_evaluate(x,&assignment);let expected=eval(guard)&&eval(source);assert_eq!(eval(complete.properties[&vec![0xd800]]),expected);assert_eq!(eval(complete.items[&7]),expected);checked+=2;}
 }}}
 println!("ANNOTATION_TRUTH_ASSERTIONS {checked}");
}

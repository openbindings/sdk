
    #[test] fn review_candidate_bridge_collisions_and_callbacks() {
        for count in [0,1,15,16,17,33] {
            let text=format!("{{{}}}",(0..count).map(|n|format!(r#""key{n}":{n}"#)).collect::<Vec<_>>().join(","));
            let object=with_hash(&text, |_|0);
            for text in [r#""absent""#,r#""key0""#,r#""key16""#] {
                let key=crate::JsonValue::parse(text).unwrap();let mut calls=0;
                let candidates=crate::backend::member_candidates(object.view(),key.view(),|_|{calls+=1;Ok::<_,()>(())}).unwrap();
                let members=candidates.collect::<Vec<_>>();assert_eq!(members.len(),count);
                let exact=members.iter().filter(|m|m.name.as_string()==key.view().as_string()).count();
                assert_eq!(exact,usize::from((text==r#""key0""# && count>0)||(text==r#""key16""# && count>16)));
                for at in 0..calls{let mut n=0;let result=crate::backend::member_candidates(object.view(),key.view(),|_|{let fail=n==at;n+=1;if fail{Err(7)}else{Ok(())}});assert_eq!(result.unwrap_err(),7);}
            }
        }
    }

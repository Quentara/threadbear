use serde_json::json;
use threadbear_core::{inbox::ThreadSummary, logins::RunnerLogins, remote::decode_remote_threads, sign_in};

#[test]
fn malformed_row_and_empty_list_cannot_remove_threads_or_authorize_cleanup() {
    use threadbear_core::inbox::ThreadList;
    let fixture = serde_json::from_str(include_str!("fixtures/wire-thread-list.json")).unwrap();
    let mut threads = std::collections::HashMap::from([("kept".into(), ThreadSummary { id:"kept".into(), ..Default::default() })]);
    let list = ThreadList::decode(&fixture).expect("skip just the bad rows");
    assert_eq!(list.rows.len(), 1);
    assert!(!list.apply(&mut threads));
    assert!(threads.contains_key("kept"));
    assert!(threads.contains_key("good"));
    assert!(!ThreadList::decode(&json!({"threads":[]})).unwrap().apply(&mut threads));
    assert_eq!(threads.len(), 2);
    for invalid in [json!({}), json!({"threads":null}), json!({"threads":42})] {
        assert!(ThreadList::decode(&invalid).is_err());
    }
    assert!(ThreadList::decode(&json!({"threads":[{"id":"new"}]})).unwrap().apply(&mut threads));
    assert_eq!(threads.len(), 1);
    assert!(threads.contains_key("new"));
}

#[test]
fn nested_account_models_and_defaults_keep_their_valid_siblings() {
    let logins = RunnerLogins::from_hello(&json!({
        "accounts":[{"id":"a", "email":42, "models":[{"id":"m", "description":42, "effortLevels":["high",false]}, 42]}, false, {"id":"b"}],
        "pools":[{"id":"p","accountIds":["a",42,"b"],"icon":false}, false],
        "defaults":{"cwd":"/work","models":{"claude":"m","codex":42},"efforts":false}
    }));
    assert_eq!(logins.accounts.len(), 2);
    assert_eq!(logins.accounts[0].models.len(), 1);
    assert_eq!(logins.accounts[0].models[0].title, "m");
    assert_eq!(logins.accounts[0].models[0].effort_levels, ["high"]);
    assert_eq!(logins.pools[0].account_ids, ["a", "b"]);
    assert_eq!(logins.defaults.cwd, "/work");
    assert_eq!(logins.defaults.model_for("claude"), Some("m"));
    assert_eq!(logins.defaults.model_for("codex"), None);
}

#[test]
fn other_protocol_structs_recover_fields_and_nested_lists() {
    use threadbear_core::{highlights::{ProjectInfo, Schedule, Panel}, convex_panel::{Choices, Key}, reply_buttons::ReplyButton, slash::decode_commands};
    let projects = ProjectInfo::list(&json!([{"id":"p", "title":"Kept project", "archived":{}, "links":[{"title":42,"url":"https://example.com"},false]}, false]));
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "Kept project");
    assert_eq!(projects[0].links.len(), 1);
    let schedules = Schedule::list(&json!([{"id":"s","title":"Kept schedule","runs":[],"lastRun":{"at":42,"outcome":"sent"}},false]));
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].last_run.as_ref().unwrap().outcome, "sent");
    let panel = Panel::from_value(&json!({"id":"p", "title":"Panel", "badge":[], "convex":{"deployment":"kept","team":42}})).unwrap();
    assert_eq!(panel.convex.unwrap().deployment, "kept");
    let choices: Choices = serde_json::from_value(json!({"deployments":[{"deployment":"kept","label":42},false],"note":42})).unwrap();
    assert_eq!(choices.deployments.len(), 1);
    assert_eq!(choices.deployments[0].deployment, "kept");
    assert!(serde_json::from_value::<Key>(json!({"deployment":"x","deploymentUrl":"https://example.com","adminKey":false})).is_err(), "credentials stay strict");
    let button = ReplyButton::decode(&json!({"id":"b","title":"Button","createdAt":false,"start":{"text":"Kept prompt","title":42}})).unwrap();
    assert_eq!(button.start.unwrap().text, "Kept prompt");
    let commands = decode_commands(&json!({"commands":[{"name":"kept","description":42},false]}));
    assert_eq!(commands[0].name, "kept");
}

#[test]
fn browser_tabs_skip_bad_rows_and_keep_other_host_fields() {
    use threadbear_core::browserbear::{agent_state, PanelState};
    let state = agent_state(br#"{"port":123,"socket":"socket","tabs":[{"tabId":1,"url":"https://example.com","title":42},false,{"tabId":2}]}"#).unwrap();
    assert_eq!(state.port, 123);
    assert_eq!(state.tabs.len(), 2);
    assert_eq!(state.tabs[0].url, "https://example.com");
    for tabs in [json!(false),json!({"nested":"wrong"}),json!(42),json!(null),json!(1.2),json!("wrong")] {
        let state = agent_state(serde_json::to_string(&json!({"port":123,"tabs":tabs})).unwrap().as_bytes()).unwrap();
        assert_eq!(state.port, 123);
        assert!(state.tabs.is_empty());
    }
    let panel: PanelState = serde_json::from_value(json!({"tabs":[false,{"tabId":1,"title":42}]})).unwrap();
    assert_eq!(panel.tabs.len(), 1);
    let mut rows = vec![json!(false); threadbear_core::browserbear::MAX_TABS];
    rows.push(json!({"tabId":1}));
    let panel: PanelState = serde_json::from_value(json!({"tabs":rows})).unwrap();
    assert!(panel.tabs.is_empty(), "the cap counts consumed entries, including bad ones");
}

#[test]
fn invalid_identities_and_duplicate_rows_do_not_authorize_cleanup() {
    use threadbear_core::inbox::ThreadList;
    for row in [json!({}),json!({"id":""}),json!({"id":42}),json!(false),json!(null)] {
        assert!(serde_json::from_value::<ThreadSummary>(row).is_err());
    }
    let list = ThreadList::decode(&json!({"threads":[{"id":"same"},{"id":"same"}]})).unwrap();
    let mut threads = std::collections::HashMap::from([("kept".into(), ThreadSummary::default())]);
    assert!(!list.apply(&mut threads));
    assert!(threads.contains_key("kept"));
    let empty = ThreadList::decode(&json!({"threads":[false,{"id":42}]})).unwrap();
    assert!(!empty.apply(&mut threads));
    assert!(threads.contains_key("kept"));
}

#[test]
fn remote_partial_invalid_and_unready_indexes_cannot_remove_missing_threads() {
    let good = json!({"runnerId":"cloud","threadId":"12345678-1234-1234-1234-123456789abc"});
    for value in [json!({}), json!({"threads":false}), json!({"threads":[false]}), json!({"threads":[good.clone(),false]}), json!({"threads":[good.clone(),good.clone()]})] {
        assert!(!decode_remote_threads(&value).can_remove_missing());
    }
    assert!(decode_remote_threads(&json!({"threads":[good]})).can_remove_missing());
    assert!(decode_remote_threads(&json!({"threads":[], "ready":true})).can_remove_missing(), "a confirmed empty remote index removes its last deleted thread");
    assert!(decode_remote_threads(&json!({"threads":[]})).can_remove_missing(), "old runners omit ready once the list has arrived");
    assert!(!decode_remote_threads(&json!({"threads":[], "ready":false})).can_remove_missing());
}

#[test]
fn decode_diagnostics_never_include_values_or_map_keys() {
    static MESSAGES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    threadbear_core::wire::set_logger(|message| MESSAGES.lock().unwrap().push(message.to_owned()));
    #[derive(serde::Deserialize)]
    struct Probe {
        #[serde(deserialize_with = "threadbear_core::wire::field")]
        _number: Option<i128>,
        #[serde(deserialize_with = "threadbear_core::wire::field")]
        _other: Option<i128>,
    }
    for _ in 0..20 {
        let _: Probe = serde_json::from_value(json!({"_number":"PRIVATE SENTINEL", "_other":"PRIVATE SENTINEL"})).unwrap();
    }
    let _: ThreadSummary = serde_json::from_value(json!({"id":"t", "contextTokens":"PRIVATE SENTINEL"})).unwrap();
    let _ = threadbear_core::inbox::ThreadList::decode(&json!({"threads":[{"id":{"PRIVATE SENTINEL":42}}]}));
    let _ = RunnerLogins::from_hello(&json!({"defaults":{"models":{"PRIVATE SENTINEL":false}}}));
    let messages = MESSAGES.lock().unwrap();
    assert!(messages.iter().any(|m| m.contains("dropped invalid")));
    assert!(messages.iter().all(|m| !m.contains("PRIVATE SENTINEL")));
    let probe: Vec<_> = messages.iter().filter(|m| m.contains("i128")).collect();
    assert_eq!(probe.len(), 2, "each field is identified separately and repeated decoding logs only once");
    assert!(probe[0].contains("wire_recovery.rs:"), "the log identifies the source field");
}

#[test]
fn provider_commands_are_quoted_as_arguments_and_old_hellos_have_defaults() {
    let old = RunnerLogins::from_hello(&json!({}));
    assert_eq!(old.login_command("claude"), "claude auth login");
    assert_eq!(old.login_command("codex"), "codex login");
    assert_eq!(old.login_command("cursor"), "agent login");
    let malformed = RunnerLogins::from_hello(&json!({"providers":[{"id":"cursor","loginCommand":false},false]}));
    assert_eq!(malformed.login_command("cursor"), "agent login");
    let account = sign_in::SignInAccount { id:"cursor-main".into(), provider:"cursor".into(), title:"Cursor".into(), config_dir:String::new() };
    let script = sign_in::script_with_command(&account, "", "agent login; touch /tmp/unwanted $(whoami)");
    assert!(script.contains("agent 'login;' touch /tmp/unwanted '$(whoami)'"));
    let unknown = RunnerLogins::from_hello(&json!({"providers":[{"id":"new","loginCommand":""},{"id":"cursor","loginCommand":""}]}));
    assert_eq!(unknown.login_command("new"), "");
    assert_eq!(unknown.login_command("cursor"), "");
    assert_eq!(old.login_command("unknown"), "");
    assert!(!sign_in::script_with_command(&account, "/opt/agent", "").contains("/opt/agent"));
}

#[test]
fn patch_trust_disclosures_and_nonmain_login_folders_fail_closed() {
    use threadbear_core::highlights::PatchInfo;
    for field in ["helpers", "grants"] {
        for bad in [json!(["readable",false]),json!({"path":"hidden"})] {
            let mut patch = json!({"id":"p","untrusted":"Read first","file":"patch.ts","hash":"hash"});
            patch[field] = bad;
            assert!(serde_json::from_value::<PatchInfo>(patch).is_err());
        }
    }
    for field in ["source", "file", "hash", "untrusted"] {
        let mut patch = json!({"id":"p","source":"project","untrusted":"Read first","file":"patch.ts","hash":"hash"});
        patch[field] = json!(false);
        assert!(serde_json::from_value::<PatchInfo>(patch).is_err());
    }
    let valid = json!({"id":"p","source":"claude","untrusted":"Read first","file":"patch.ts","hash":"hash"});
    assert!(serde_json::from_value::<PatchInfo>(valid.clone()).unwrap().can_trust());
    for field in ["source", "file", "hash"] {
        let mut patch = valid.clone();
        patch.as_object_mut().unwrap().remove(field);
        assert!(!serde_json::from_value::<PatchInfo>(patch).unwrap().can_trust());
    }
    for (id, provider) in [("claude-1","claude"),("codex-1","codex")] {
        let logins = RunnerLogins::from_hello(&json!({"accounts":[{"id":id,"provider":provider,"configDir":false}]}));
        let a = &logins.accounts[0];
        let account = sign_in::SignInAccount { id:a.id.clone(),provider:a.provider.clone(),title:a.title.clone(),config_dir:a.config_dir.clone() };
        assert!(sign_in::account_for(provider,false,Some(account.clone())).is_none());
        let script = sign_in::script(&account,"/opt/cli");
        assert!(!script.contains("/opt/cli"));
    }
    let malformed = RunnerLogins::from_hello(&json!({"accounts":[{"id":"codex-1","provider":{},"configDir":"/codex"}]}));
    assert!(malformed.accounts.is_empty(), "a bad provider cannot change which CLI signs in");
    let mismatch = sign_in::SignInAccount { id:"codex-1".into(), provider:"claude".into(), title:String::new(), config_dir:"/codex".into() };
    assert!(sign_in::account_for("codex",false,Some(mismatch)).is_none());
}

#[test]
fn malformed_summary_fields_keep_the_thread_and_good_nested_rows() {
    let t: ThreadSummary = serde_json::from_value(json!({
        "id":"good", "title":"Keep this thread", "contextTokens":"bad",
        "sample":[], "projects":[{"id":"p","title":"Project","pinned":"bad"}, 42],
        "asks":[{"action":"Choose", "suggested":"bad", "options":["Yes",42,"No"]}, false],
        "carryOnAtReset":"bad", "futureField":42
    })).expect("one malformed field cannot hide a thread");
    assert_eq!(t.title, "Keep this thread");
    assert_eq!(t.context_tokens, None);
    assert_eq!(t.projects.as_ref().unwrap().len(), 1);
    assert_eq!(t.projects.unwrap()[0].title, "Project");
    let asks = t.asks.unwrap();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].options, None, "never shift choice positions past a bad label");
    assert!(t.carry_on_at_reset);
}

#[test]
fn remote_summaries_keep_search_words_and_creation_date() {
    let remote = decode_remote_threads(&json!({"threads":[{
        "runnerId":"cloud", "threadId":"12345678-1234-1234-1234-123456789abc",
        "title":"A thread", "keywords":"unusual-search-word", "createdAt":"2026-11-01T10:00:00Z"
    }]}));
    let t = &remote.threads[0];
    assert!(threadbear_core::inbox::matches(t, "unusual-search-word"));
    assert_eq!(t.created_at.as_deref(), Some("2026-11-01T10:00:00Z"));
}

#[test]
fn provider_sign_in_uses_hello_login_command() {
    let logins = RunnerLogins::from_hello(&json!({"providers":[{"id":"cursor", "loginCommand":"agent login"}]}));
    let account = sign_in::SignInAccount { id:"cursor-main".into(), provider:"cursor".into(), title:"Cursor".into(), config_dir:String::new() };
    let script = sign_in::script_with_command(&account, "/opt/agent", logins.login_command("cursor"));
    assert!(script.contains("'/opt/agent' login\n"), "{script}");
    assert!(!script.contains("auth login"));
}

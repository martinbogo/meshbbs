use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;

fn msgs_for<'a>(msgs: &'a Vec<(String, String)>, node: &str) -> Vec<&'a String> {
    msgs.iter().filter(|(to, _)| to == node).map(|(_, m)| m).collect()
}

#[tokio::test]
async fn thread_read_is_chunked_and_prompt_on_last() {
    // Arrange: small frame budget to force chunking during read view
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    cfg.storage.max_message_size = 20; // extremely small to guarantee chunking regardless of content
    let mut server = BbsServer::new(cfg).await.expect("server");

    // Create a topic and a long post
    server
        .test_create_topic(
            "hello",
            "Hello Topic",
            "Topic for chunk test",
            0,
            0,
            "sysop",
        )
        .await
        .expect("create topic");
    // Compose a message with a short title + modest body (<= max_message_size),
    // but when combined with header+footer it will exceed the budget to trigger chunking.
    let body = "This body is short but when combined with header and footer it will exceed the small frame budget.";
    // Keep total content (title + blank + body) under the 100-byte storage cap
    let content = format!("Test Title\n\n{}", &body[..60]);
    let _id = server
        .test_store_message("hello", "alice", &content)
        .await
        .expect("store long message");

    let node = "n1";

    // Act: login and navigate to read the thread (M -> 1 -> 1)
    server
        .route_test_text_direct(node, "LOGIN alice")
        .await
        .expect("login");
    server
        .route_test_text_direct(node, "M")
        .await
        .expect("topics");
    // Determine the numeric selection for the 'hello' topic based on current ordering
    let _ = server
        .test_get_messages("general", 1) // force storage init; not strictly needed
        .await
        .ok();
    let all_topics = server
        .test_list_topics()
        .await
        .expect("list topics");
    let pos = all_topics
        .iter()
        .position(|t| t == "hello")
        .expect("hello topic present");
    let target_page = pos / 5 + 1;
    let select_num = (pos % 5) + 1; // 1..5
    // Advance pages if needed
    for _ in 1..target_page {
        server
            .route_test_text_direct(node, "L")
            .await
            .expect("next page");
    }
    server
        .route_test_text_direct(node, &select_num.to_string())
        .await
        .expect("threads");
    let before = msgs_for(server.test_messages(), node).len();
    server
        .route_test_text_direct(node, "1")
        .await
        .expect("read thread");
    let msgs = msgs_for(server.test_messages(), node);
    let after = msgs.len();
    assert!(after > before, "expected new messages to be sent when reading");

    // Find the new messages from the read action
    let new_msgs = &msgs[before..after];
    assert!(new_msgs.len() >= 2, "expected multiple chunks for long read (got {})", new_msgs.len());

    // Intermediate chunks should not end with the prompt; final chunk should include it
    // Prompt format while reading: "alice@hello>"
    let prompt = "alice@hello>";
    for (i, m) in new_msgs.iter().enumerate() {
        let is_last = i + 1 == new_msgs.len();
        if is_last {
            assert!(m.ends_with(prompt), "final chunk must end with prompt; got: {}", m);
        } else {
            assert!(
                !m.ends_with(prompt),
                "intermediate chunk should not include prompt; chunk: {}",
                m
            );
        }
        // Budget guard
    assert!(m.as_bytes().len() <= 20, "chunk exceeds 20 bytes ({}): {}", m.as_bytes().len(), m);
    }
}

// Pins how `[image N]` placeholders in the input buffer interact with
// slash-command parsing in submit_input (crates/jcode-tui/src/tui/app/input.rs).
//
// Placeholders are plain text: expand_paste_placeholders only expands
// `[pasted N lines]` markers, never `[image N]`. Command routing therefore
// sees the placeholder literally.

fn attach_test_image(app: &mut App) {
    // Mirrors attach_image() in input.rs: push pending image, insert
    // "[image N]" placeholder at the cursor.
    app.pending_images
        .push(("image/png".to_string(), "aGVsbG8=".to_string()));
    let placeholder = format!("[image {}]", app.pending_images.len());
    let mut input = app.input().to_string();
    let pos = input.len();
    input.insert_str(pos, &placeholder);
    app.set_input_for_test(input);
}

#[test]
fn test_image_placeholder_before_text_submits_as_user_turn_with_image() {
    let mut app = create_test_app();
    attach_test_image(&mut app);
    let prompt = format!("{} describe this", app.input());
    app.set_input_for_test(prompt.clone());

    app.submit_input();

    assert!(app.is_processing, "placeholder + text should start a turn");
    assert!(
        app.pending_images.is_empty(),
        "submitting must consume pending images"
    );
    let submitted = app.session.messages.last().expect("submitted message");
    assert!(
        matches!(submitted.content.first(), Some(ContentBlock::Image { .. })),
        "image block must be attached"
    );
    assert!(matches!(
        submitted.content.last(),
        Some(ContentBlock::Text { text, .. }) if text == &prompt
    ));
}

#[test]
fn test_image_placeholder_prefix_prevents_slash_command_routing() {
    let mut app = create_test_app();
    attach_test_image(&mut app);
    let input = format!("{}/help", app.input());
    app.set_input_for_test(input);

    app.submit_input();

    // Input does not start with '/', so it is a normal user turn (with the
    // image attached), not a /help invocation.
    assert!(app.is_processing);
    assert!(app.help_scroll.is_none(), "help must not open");
    assert!(app.pending_images.is_empty());
}

#[test]
fn test_slash_command_with_trailing_image_placeholder_routes_as_command() {
    let mut app = create_test_app();
    app.set_input_for_test("/help ");
    attach_test_image(&mut app);
    assert_eq!(app.input(), "/help [image 1]");

    app.submit_input();

    // "/help [image 1]" is parsed as `/help <topic>` with the literal
    // placeholder as topic, so it reports an unknown command and the
    // pending image stays attached in the app (not sent, not dropped).
    assert!(!app.is_processing, "command routing must not start a turn");
    let last = app.display_messages().last().expect("display message");
    assert_eq!(last.role, "error");
    assert!(
        last.content.contains("Unknown command"),
        "unexpected message: {}",
        last.content
    );
    assert_eq!(
        app.pending_images.len(),
        1,
        "handled command leaves the pending image queued"
    );
}

#[test]
fn test_unknown_skill_with_image_placeholder_reports_error_and_keeps_image() {
    let mut app = create_test_app();
    app.set_input_for_test("/definitely-not-a-skill ");
    attach_test_image(&mut app);

    app.submit_input();

    assert!(!app.is_processing);
    let last = app.display_messages().last().expect("display message");
    assert_eq!(last.role, "error");
    assert!(
        last.content.contains("Unknown skill"),
        "unexpected message: {}",
        last.content
    );
    assert_eq!(app.pending_images.len(), 1);
}

// A submitted image must be visible to the user, not just sent to the model.
// Before this echo the transcript only carried the literal `[image N]` text,
// so the user had no way to see what they actually attached.

fn user_echoed_images(app: &App) -> Vec<&crate::session::RenderedImage> {
    app.remote_side_pane_images
        .iter()
        .filter(|image| image.source == crate::session::RenderedImageSource::UserInput)
        .collect()
}

#[test]
fn submitted_image_is_echoed_inline_anchored_to_its_own_user_prompt() {
    let mut app = create_test_app();
    attach_test_image(&mut app);
    app.set_input_for_test(format!("{} describe this", app.input()));

    app.submit_input();

    let echoed = user_echoed_images(&app);
    assert_eq!(
        echoed.len(),
        1,
        "the submitted image must be echoed into the transcript"
    );
    assert_eq!(echoed[0].media_type, "image/png");
    assert_eq!(
        echoed[0].data, "aGVsbG8=",
        "the echoed payload must be the bytes that were actually sent"
    );
    assert_eq!(
        echoed[0].anchor,
        Some(crate::session::RenderedImageAnchor::UserPrompt { ordinal: 0 }),
        "the first user prompt anchors at ordinal 0, so the image renders under it"
    );
}

#[test]
fn second_submitted_image_anchors_to_the_second_prompt_not_the_first() {
    let mut app = create_test_app();

    attach_test_image(&mut app);
    app.set_input_for_test(format!("{} first", app.input()));
    app.submit_input();

    // A second prompt must not stack its image under prompt #1; the ordinal
    // has to advance with the rendered user-prompt count.
    app.pending_images
        .push(("image/jpeg".to_string(), "c2Vjb25k".to_string()));
    app.set_input_for_test("second [image 1]".to_string());
    app.submit_input();

    let echoed = user_echoed_images(&app);
    assert_eq!(echoed.len(), 2, "both submitted images must be echoed");
    assert_eq!(
        echoed[0].anchor,
        Some(crate::session::RenderedImageAnchor::UserPrompt { ordinal: 0 })
    );
    assert_eq!(
        echoed[1].anchor,
        Some(crate::session::RenderedImageAnchor::UserPrompt { ordinal: 1 }),
        "the second prompt's image must anchor to the second prompt"
    );
    assert_eq!(echoed[1].data, "c2Vjb25k");
}

#[test]
fn text_only_submission_echoes_no_inline_image() {
    let mut app = create_test_app();
    app.set_input_for_test("just text".to_string());

    app.submit_input();

    assert!(
        user_echoed_images(&app).is_empty(),
        "a text-only turn must not fabricate an inline image"
    );
}

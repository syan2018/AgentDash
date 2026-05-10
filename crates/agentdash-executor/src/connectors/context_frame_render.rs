use agentdash_spi::hooks::ContextFrame;

pub(crate) fn render_context_frames_to_text(frames: &[ContextFrame]) -> String {
    frames
        .iter()
        .filter_map(|frame| {
            let text = frame.rendered_text.trim();
            (!text.is_empty()).then(|| text.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub(crate) fn compose_prompt_text(user_text: &str, frames: &[ContextFrame]) -> String {
    let frames_text = render_context_frames_to_text(frames);
    let user_text = user_text.trim();
    match (frames_text.is_empty(), user_text.is_empty()) {
        (true, true) => String::new(),
        (false, true) => frames_text,
        (true, false) => user_text.to_string(),
        (false, false) => format!("{frames_text}\n\n{user_text}"),
    }
}

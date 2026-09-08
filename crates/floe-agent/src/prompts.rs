pub fn calendar_briefing_prompt(focus_minutes: u16) -> String {
    render_focus_minutes(
        include_str!("../prompts/calendar_briefing.txt"),
        focus_minutes,
    )
}

pub fn calendar_focus_proposal_prompt(focus_minutes: u16) -> String {
    render_focus_minutes(
        include_str!("../prompts/calendar_focus_proposal.txt"),
        focus_minutes,
    )
}

pub fn fixture_today_prompt() -> &'static str {
    include_str!("../prompts/fixture_today.txt").trim()
}

pub fn fixture_follow_up_prompt() -> &'static str {
    include_str!("../prompts/fixture_follow_up.txt").trim()
}

pub fn fixture_repeated_call_prompt() -> &'static str {
    include_str!("../prompts/fixture_repeated_call.txt").trim()
}

pub fn fixture_unavailable_prompt() -> &'static str {
    include_str!("../prompts/fixture_unavailable.txt").trim()
}

fn render_focus_minutes(template: &str, focus_minutes: u16) -> String {
    template
        .trim()
        .replace("{{focus_minutes}}", &focus_minutes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_calendar_templates_render_without_placeholders() {
        assert_eq!(
            calendar_briefing_prompt(60),
            "Brief today's calendar and find a 60-minute focus window."
        );
        assert_eq!(
            calendar_focus_proposal_prompt(45),
            "Propose a 45-minute focus block from today's calendar."
        );
    }
}

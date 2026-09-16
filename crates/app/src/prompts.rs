//! The scripted model answers the app fixture replays.

pub fn fixture_today_prompt() -> &'static str {
    include_str!("../prompts/fixture_today.txt")
}

pub fn fixture_follow_up_prompt() -> &'static str {
    include_str!("../prompts/fixture_follow_up.txt")
}

pub fn fixture_repeated_call_prompt() -> &'static str {
    include_str!("../prompts/fixture_repeated_call.txt")
}

pub fn fixture_unavailable_prompt() -> &'static str {
    include_str!("../prompts/fixture_unavailable.txt")
}

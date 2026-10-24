use skein_oauth_accounts_world::sign_in::{self, Story};
const STORIES: &[Story] = &[
    Story::WrongPath,
    Story::WrongHost,
    Story::LongHead,
    Story::CloseWaiting,
    Story::Cancel,
    Story::Abort,
    Story::Confidential,
    Story::Return(sign_in::Return::Back),
    Story::Return(sign_in::Return::Replace),
    Story::Return(sign_in::Return::WrongPath),
    Story::Return(sign_in::Return::After),
    Story::Paste,
    Story::PasteThenListener,
    Story::ListenerThenPaste,
    Story::PasteTwice,
    Story::PasteWrong,
    Story::StateRefusals,
    Story::ListenerWrongState,
    Story::PasteExpired,
    Story::PasteCancelled,
    Story::PasteClose,
    Story::Public,
];
#[test]
fn browser_arrival_wrong_redirects_and_close_replay_under_short_operations() {
    for seed in 0..32 {
        let story = STORIES[usize::try_from(seed).expect("replay seed") % STORIES.len()];
        let first = sign_in::run(seed, story, true, false, skein_world::Memory::Unchecked);
        let second = sign_in::run(seed, story, true, false, skein_world::Memory::Unchecked);
        sign_in::assert_replay(&first, &second);
    }
}

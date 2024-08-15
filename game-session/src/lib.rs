#![no_std]

use game_session_io::*;
use gstd::{exec, msg, debug, vec};

const TRIES_LIMIT: u8 = 5;

static mut GAME_SESSION_STATE: Option<GameSession> = None;

#[no_mangle]
extern "C" fn init() {
    let game_session_init: GameSessionInit =
        msg::load().expect("Unable to decode `GameSessionInit`");
    game_session_init.assert_valid();
    unsafe { GAME_SESSION_STATE = Some(game_session_init.into()) };
}

#[no_mangle]
extern "C" fn handle() {
    let game_session_action: GameSessionAction =
        msg::load().expect("Unable to decode `GameSessionAction`");
    let game_session = unsafe {
        GAME_SESSION_STATE
            .as_mut()
            .expect("Game is not initialized")
    };
    match game_session_action {
        GameSessionAction::StartGame => {
            let user = msg::source();
            let session_info = game_session.sessions.entry(user).or_default();
            debug!("handle:{:?}",session_info);
            match &session_info.session_status {
                SessionStatus::Init | SessionStatus::GameOver(..) | SessionStatus::WaitWordleStartReply => {
                    let send_to_wordle_msg_id = msg::send(
                        game_session.wordle_program_id,
                        WordleAction::StartGame { user },
                        0,
                    )
                    .expect("Error in sending a message");

                    session_info.session_id = msg::id();
                    session_info.original_msg_id = msg::id();
                    session_info.send_to_wordle_msg_id = send_to_wordle_msg_id;
                    session_info.tries = 0;
                    session_info.session_status = SessionStatus::WaitWordleStartReply;

                    msg::send_delayed(
                        exec::program_id(),
                        GameSessionAction::CheckGameStatus {
                            user,
                            session_id: msg::id(),
                        },
                        0,
                        200,
                    )
                    .expect("Error in send_delayed a message");
                    exec::wait();
                }
                SessionStatus::WaitUserInput | SessionStatus::WaitWordleCheckWordReply => {
                    panic!("The user is already in a game");
                }
                _ => {}
            }
        }
        GameSessionAction::CheckWord { word } => {
            let user = msg::source();
            let session_info = game_session.sessions.entry(user).or_default();
            match &session_info.session_status {
                SessionStatus::WaitUserInput => {
                    assert!(
                        word.len() == 5 && word.chars().all(|c| c.is_lowercase()),
                        "Invalid word"
                    );
                    let send_to_wordle_msg_id = msg::send(
                        game_session.wordle_program_id,
                        WordleAction::CheckWord { user, word },
                        0,
                    )
                    .expect("Error in sending a message");

                    session_info.original_msg_id = msg::id();
                    session_info.send_to_wordle_msg_id = send_to_wordle_msg_id;
                    session_info.session_status = SessionStatus::WaitWordleCheckWordReply;

                    exec::wait();
                }
                _ => {
                    panic!("Invalid state or the user is not in the game");
                }
            }
        }
        GameSessionAction::CheckGameStatus { user, session_id } => {
            if msg::source() == exec::program_id() {
                if let Some(session_info) = game_session.sessions.get_mut(&user) {
                    if session_id == session_info.session_id
                        && !matches!(session_info.session_status, SessionStatus::GameOver(..))
                    {
                        session_info.session_status = SessionStatus::GameOver(GameStatus::Lose);
                        msg::send(user, GameSessionEvent::GameOver(GameStatus::Lose), 0)
                            .expect("Error in sending a reply");
                    }
                }
            }
        }
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    let reply_to = msg::reply_to().expect("Failed to query reply_to data");
    let wordle_event: WordleEvent = msg::load().expect("Unable to decode `WordleEvent`");
    let game_session = unsafe {
        GAME_SESSION_STATE
            .as_mut()
            .expect("Game is not initialized")
    };

    let user = wordle_event.get_user();
    debug!("wordle:{:?}",wordle_event);

    if let Some(session_info) = game_session.sessions.get_mut(user) {
        if reply_to == session_info.send_to_wordle_msg_id  {
            debug!("{:?}:::{:?}",reply_to,session_info.original_msg_id);
            session_info.session_status = SessionStatus::ReplyReceived(wordle_event.clone());
            debug!("{:?}",session_info.session_status);
            match wordle_event {
                WordleEvent::GameStarted { .. } => {
                    session_info.session_status = SessionStatus::WaitUserInput;
                }
                WordleEvent::WordChecked {
                    ref correct_positions,
                    contained_in_word: _,
                    ..
                } => {
                    session_info.tries += 1;
                    if *correct_positions == vec![0, 1, 2, 3, 4] {
                        session_info.session_status = SessionStatus::GameOver(GameStatus::Win);
                        msg::reply(GameSessionEvent::GameOver(GameStatus::Win), 0)
                            .expect("Error in sending a reply");
                    } else if session_info.tries >= TRIES_LIMIT {
                        session_info.session_status = SessionStatus::GameOver(GameStatus::Lose);
                        msg::reply(GameSessionEvent::GameOver(GameStatus::Lose), 0)
                            .expect("Error in sending a reply");
                    } else {
                       session_info.session_status = SessionStatus::WaitUserInput;
                       debug!("else:{:?}",session_info.session_status);
                        msg::reply::<GameSessionEvent>((&wordle_event).into(), 0)
                            .expect("Error in sending a reply");
                    }
                }
            }
        }
    debug!("131313");
    exec::wake(session_info.original_msg_id).expect("Failed to wake message");
    }
}

#[no_mangle]
extern "C" fn state() {
    let game_session = unsafe {
        GAME_SESSION_STATE
            .as_ref()
            .expect("Game is not initialized")
    };
    msg::reply::<GameSessionState>(game_session.into(), 0)
        .expect("failed to encode or reply from `state()`");
}

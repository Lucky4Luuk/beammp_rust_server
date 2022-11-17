use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};

use tokio::sync::mpsc::{Sender, Receiver};

#[derive(PartialEq)]
pub enum ServerEvent {
    ClientListUpdate(Vec<(u8, crate::server::UserData)>),
}

#[derive(PartialEq)]
pub enum ServerCommand {

}

#[derive(PartialEq)]
pub enum UpdateResult {
    Continue,
    Exit,
}

pub struct Communicator {
    pub id_name_list: Vec<(u8, crate::server::UserData)>,

    rx_event: Receiver<ServerEvent>,
    tx_cmd: Sender<ServerCommand>,
}

impl Communicator {
    pub fn new(rx_event: Receiver<ServerEvent>, tx_cmd: Sender<ServerCommand>) -> Self {
        Self {
            id_name_list: Vec::new(),
            rx_event: rx_event,
            tx_cmd: tx_cmd,
        }
    }

    pub fn handle_input(&mut self, input: KeyEvent) -> UpdateResult {
        match input.code {
            KeyCode::Char('q') if input.modifiers.contains(KeyModifiers::CONTROL) => {
                return UpdateResult::Exit;
            },
            KeyCode::Char('c') => {
                info!("C pressed!");
            },
            _ => {},
        }

        UpdateResult::Continue
    }

    pub fn tick(&mut self) -> UpdateResult {
        // TODO: Read events here

        UpdateResult::Continue
    }
}

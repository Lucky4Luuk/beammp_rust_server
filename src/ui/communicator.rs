use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};

#[derive(PartialEq)]
pub enum ServerEvent {

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

}

impl Communicator {
    pub fn new() -> Self {
        Self {

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
        UpdateResult::Continue
    }
}

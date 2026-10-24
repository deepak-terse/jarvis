//! How a turn enters and leaves: the terminal and push-to-talk voice. The agent never
//! knows which one is in use; channels differ only in the `Sink` they pass it.

pub mod terminal;
pub mod voice;

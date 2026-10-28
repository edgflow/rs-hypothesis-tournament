//! A hypothesis tournament. Competing explanations for a failing test become
//! bets on how often the test fails under each condition; experiments decide
//! which bet was right. The AI writes the bets, Rust runs the experiments, and
//! the ranking comes from evidence only, never from how confident the AI was.

pub mod case;
pub mod config;
pub mod generate;
pub mod hypothesis;
pub mod openai;
pub mod score;
pub mod style;
pub mod tournament;
pub mod world;

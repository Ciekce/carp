//! # UCI integration
use std::{ 
    io, io::BufRead, 
    thread, sync, sync::atomic
};

use crate::{
    board_repr::Board,
    tables::Tables,
    search::Search,
    tt::TT,
};

const ENGINE_ID: &str = "id name Carp 0.1\nid author Andrea S.";
// add options here
const ENGINE_OPTIONS:&str = "";

// uci implementation inspired by weiawaga itself inspired by asymptote
pub struct UCIController {
    stop: sync::Arc<sync::atomic::AtomicBool>,
    _engine_thread: thread::JoinHandle<()>,
    thread_tx: sync::mpsc::Sender<UCICommand>,
}

/// # UCI Controller
/// 
/// Handles communication with the gui and communicates with main engine thread
impl UCIController {
    pub fn new() -> UCIController {
        let (tx, rx) = sync::mpsc::channel::<UCICommand>();
        let stop = sync::Arc::new(atomic::AtomicBool::new(false));

        UCIController {
            stop: stop.clone(),
            _engine_thread: thread::spawn(move || {
                UCIEngine::new(rx, stop).run()
            }),
            thread_tx: tx,
        }
    }

    pub fn run(&self) {
        println!("{}", ENGINE_ID);
        println!("{}", ENGINE_OPTIONS);

        let stream = io::stdin().lock();

        for line in stream
                        .lines()
                        .map(|l| l.expect("Parsing error!"))
        {
            match UCICommand::try_from(line.as_ref()) {
                Ok(command) => {
                    match command {
                        UCICommand::Uci => {
                            println!("{}", ENGINE_ID);
                            println!("{}", ENGINE_OPTIONS);
                            println!("uciok");
                        },
                        UCICommand::IsReady => {
                            println!("readyok");
                        },
                        UCICommand::Stop => self.stop.store(true, atomic::Ordering::SeqCst), // strict ordering
                        UCICommand::Quit => return,
                        _ => self.thread_tx.send(command).unwrap(),
                    }
                },
                Err(e) => eprintln!("{}", e),
            };
        }
    }
}

enum UCICommand {
    UciNewGame,
    Uci,
    IsReady,
    Position(Board, Vec<String>),
    Go(u8),
    Option,
    Quit,
    Stop,
}

impl TryFrom<&str> for UCICommand {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut tokens = value.split_whitespace();

        match tokens.next() {
            Some("ucinewgame") => Ok(Self::UciNewGame),
            Some("uci") => Ok(Self::Uci),
            Some("option") => Ok(Self::Option),
            Some("isready") => Ok(Self::IsReady),
            Some("stop") => Ok(Self::Stop),
            Some("quit") => Ok(Self::Quit),
            Some("go") => {
                match tokens.next() {
                    Some("depth") => {
                        match tokens
                            .next()
                            .ok_or("Unspecified depth!")?
                            .parse()
                        {
                            Ok(depth) => Ok(Self::Go(depth)),
                            Err(_) =>Err("Could not parse depth"),
                        }
                    },
                    _ => Ok(Self::Go(5)),
                }
            }
            Some("position") => {
                let board = match tokens.next() {
                    Some("startpos") => Board::default(),
                    Some("fen") => {
                        let fen = &tokens
                            .clone()
                            .take(6)
                            .collect::<Vec<&str>>()
                            .join(" ")
                            [..];
                
                        for _ in 0..6 { tokens.next(); } // advance iterator

                        Board::try_from(fen)?
                    },
                    _ => return Err("Invalid position command"),
                };

                let mut moves = Vec::new();
                if let Some("moves") = tokens.next() {
                    moves = tokens
                        .map(String::from)
                        .collect();
                }

                Ok(Self::Position(board, moves))
            }
            _ => Err("Error parsing command!"),
        }
    }
}

/// # UCI Chess engine
/// 
/// Sets up positions and dispatches searches. The search itself is responsible for the stop
/// command.
struct UCIEngine {
    board: Board,
    tables: Tables,
    tt: TT,
    controller_rx: sync::mpsc::Receiver<UCICommand>,
    stop: sync::Arc<sync::atomic::AtomicBool>,
}

impl UCIEngine {
    pub fn new(
        rx: sync::mpsc::Receiver<UCICommand>,
        stop: sync::Arc<sync::atomic::AtomicBool>,
    ) -> UCIEngine {
        UCIEngine{
            board: Board::default(),
            tables: Tables::default(),
            tt: TT::default(),
            controller_rx: rx,
            stop,
        }
    }
    
    pub fn run(&mut self) {
        for command in &self.controller_rx {
            match command {
                UCICommand::UciNewGame => {
                    self.board = Board::default();
                    self.tt = TT::default();
                }
                UCICommand::Position(board, moves) => {
                    self.board = board;

                    for move_string in moves {
                        let new = self.board
                            .generate_moves(&self.tables)
                            .into_iter()
                            .find(|m| { move_string == m.to_string()});

                        match new {
                            Some(new_move) => {
                                match self.board.make_move(new_move, &self.tables) {
                                    Some(b) => self.board = b,
                                    None => eprintln!("Move is not legal!"),
                                }
                            },
                            None => eprintln!("Move is not pseudolegal!"),
                        };
                    }
                }
                UCICommand::Go(d) => {
                    let mut search = Search::new(&mut self.tt, &self.tables);
                    let best_move = search.iterative_search(&self.board, d);

                    println!("\nbestmove {}", best_move);
                }
                UCICommand::Option => continue, // temporary

                _ => eprintln!("Unexpected UCI command!"),
            }
        }
    }
}

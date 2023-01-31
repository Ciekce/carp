use std::cmp::{max, min};

use crate::{clock::Clock, evaluation::*, moves::*, position::Position, tt::*};

pub const MAX_DEPTH: usize = 128; // max depth to search at
const LMR_THRESHOLD: u32 = 4; // moves to execute before any reduction
const LMR_LOWER_LIMIT: usize = 3; // stop applying lmr near leaves
const NMP_REDUCTION: usize = 2; // null move pruning reduced depth
const ASPIRATION_WINDOW: Eval = 50; // aspiration window width
const ASPIRATION_THRESHOLD: usize = 4; // depth at which windows are reduced
const FUTILITY_MARGIN: Eval = 1100; // highest queen value possible

/// Search the move tree, starting at the given position
pub struct Search<'a> {
    position: Position,
    clock: Clock,
    tt: &'a TT,
    nodes: u64,
    seldepth: usize,
    stop: bool,
}

impl<'a> Search<'a> {
    pub fn new(position: Position, clock: Clock, tt: &'a TT) -> Search<'a> {
        Search {
            position,
            clock,
            tt,
            nodes: 0,
            seldepth: 0,
            stop: false,
        }
    }

    /// Iteratively searches the board at increasing depth
    /// After the shallower depths, we start doing reduced-window searches and eventually reopen
    /// each "side" of the window in case of fail-high or fail-low
    pub fn iterative_search(&mut self, print_info: bool) -> (Move, usize) {
        let mut best_move = NULL_MOVE;
        let mut temp_best: Move;
        let mut alpha = -MATE;
        let mut beta = MATE;
        let mut eval: Eval = 0;
        let mut depth = 1;

        while !self.stop
            && self.clock.start_check(depth)
            && !is_mate(eval.abs())
            && depth < MAX_DEPTH
        {
            (eval, temp_best) = self.search_root(alpha, beta, depth);

            if eval <= alpha {
                alpha = -MATE;
            } else if eval >= beta {
                beta = MATE;
            } else {
                if depth >= ASPIRATION_THRESHOLD {
                    alpha = eval - ASPIRATION_WINDOW;
                    beta = eval + ASPIRATION_WINDOW;
                }

                if !self.stop {
                    if print_info {
                        self.print_info(eval, depth);
                        self.seldepth = 0;
                    }
                    best_move = temp_best;
                    depth += 1;
                }
            }
        }

        (best_move, depth - 1)
    }

    // Separate function for searching the root. Saves temporary tt entries for root moves and
    // avoids a few optimizations. Allows returning the best move without pv retrieval.
    // Will be useful in case of future implementations of various
    // root-only heuristics
    fn search_root(&mut self, mut alpha: Eval, beta: Eval, mut depth: usize) -> (Eval, Move) {
        let in_check = self.position.king_in_check();
        if in_check {
            depth += 1;
        }

        // Probe tt only for best move
        match self.tt.probe(self.position.hash()) {
            Some(entry) => self.position.set_tt_move(Some(entry.get_move())),
            None => self.position.set_tt_move(None),
        };

        let mut eval: Eval;
        let mut best_move = NULL_MOVE;
        let mut tt_entry = TTField::new(&self.position, TTFlag::Upper, best_move, -MATE, depth);

        for (move_count, (m, _)) in self.position.generate_moves().enumerate() {
            self.position.make_move(m);

            if move_count == 0 {
                // full search on first move
                eval = -self.negamax(-beta, -alpha, depth - 1);
                best_move = m;
            } else {
                // use plain pvs without reductions in root
                eval = -self.negamax(-alpha - 1, -alpha, depth - 1);
                if eval > alpha && eval < beta && !self.stop {
                    eval = -self.negamax(-beta, -alpha, depth - 1);
                }
            };
            self.position.undo_move();

            if self.stop {
                return (0, NULL_MOVE);
            }

            if eval > alpha {
                best_move = m;
                alpha = eval;

                if eval >= beta {
                    if !(m.is_capture()) {
                        self.position.update_sorter(m, depth);
                    };

                    tt_entry.update_data(TTFlag::Lower, best_move, beta);
                    self.tt.insert(tt_entry);

                    return (beta, best_move);
                }

                // in root, we insert partial results for the other threads to use
                tt_entry.update_data(TTFlag::Upper, best_move, alpha);
                self.tt.insert(tt_entry);
            }
        }

        if !self.stop {
            tt_entry.update_data(TTFlag::Exact, best_move, alpha);
            self.tt.insert(tt_entry);
        }

        (alpha, best_move)
    }

    /// Fail-Hard Negamax search
    fn negamax(&mut self, mut alpha: Eval, mut beta: Eval, mut depth: usize) -> Eval {
        if self.stop || !self.clock.mid_check(self.nodes) {
            self.stop = true;
            return 0;
        }

        let pv_node = alpha != beta - 1; // False when searching with a null window
        let in_check = self.position.king_in_check();
        if in_check {
            depth += 1;
        }

        if depth == 0 {
            return self.quiescence(alpha, beta);
        }
        self.nodes += 1;

        // Mate distance pruning (it's a bit faster to do this before draw detection)
        alpha = max(-MATE + self.position.ply as Eval, alpha);
        beta = min(MATE - self.position.ply as Eval - 1, beta);
        if alpha >= beta {
            return alpha;
        }

        // Stop searching if the position is a rule-based draw
        if self.position.is_draw() {
            return 0;
        }

        // Probe tt for eval and best move
        match self.tt.probe(self.position.hash()) {
            Some(entry) => {
                let tt_move = entry.get_move();

                if entry.get_depth() >= depth {
                    let tt_eval = entry.get_value(self.position.ply);

                    match entry.get_flag() {
                        TTFlag::Exact => return tt_eval,
                        TTFlag::Upper => beta = min(beta, tt_eval),
                        TTFlag::Lower => alpha = max(alpha, tt_eval),
                    }

                    // Upper/Lower flags can cause indirect cutoffs!
                    if alpha >= beta {
                        return tt_eval;
                    }
                }
                self.position.set_tt_move(Some(tt_move));
            }

            None => self.position.set_tt_move(None),
        };

        // Apply null move pruning
        let mut eval: Eval;
        if depth > NMP_REDUCTION && !pv_node && !in_check && !self.position.only_king_pawns_left() {
            self.position.make_null();
            eval = -self.negamax(-beta, -beta + 1, depth - 1 - NMP_REDUCTION);
            self.position.undo_move();

            // cutoff above beta and not for mate scores!
            if eval >= beta && !is_mate(eval.abs()) {
                return beta;
            }
        }

        let mut moves_checked: u32 = 0;
        let mut best_move = NULL_MOVE;
        let mut tt_bound = TTFlag::Upper;

        for (m, _) in self.position.generate_moves() {
            moves_checked += 1;
            self.position.make_move(m);

            if moves_checked == 1 {
                // full depth search on first move
                eval = -self.negamax(-beta, -alpha, depth - 1);
                best_move = m; // always init at least one best move
            } else {
                // reduce depth for all moves beyond first
                if moves_checked >= LMR_THRESHOLD
                    && depth >= LMR_LOWER_LIMIT
                    && !in_check
                    && !m.is_capture()
                    && !m.is_promotion()
                {
                    // LMR with a null window
                    eval = -self.negamax(-alpha - 1, -alpha, depth - 2);
                } else {
                    eval = alpha + 1; // else force pvs
                }

                if eval > alpha {
                    // normal PVS for any move beyond the first
                    eval = -self.negamax(-alpha - 1, -alpha, depth - 1);

                    // sneaky way to also dodge re-searching when the window is already null
                    if eval > alpha && eval < beta && !self.stop {
                        // PVS failed
                        eval = -self.negamax(-beta, -alpha, depth - 1);
                    }
                }
            };

            self.position.undo_move();

            if self.stop {
                return 0;
            }

            if eval > alpha {
                if eval >= beta {
                    if !(m.is_capture()) {
                        self.position.update_sorter(m, depth);
                    };

                    alpha = beta;
                    tt_bound = TTFlag::Lower;
                    break;
                }

                alpha = eval;
                tt_bound = TTFlag::Exact;
            }
        }

        // Mate or stalemate. Don't save in the TT, this is very cheap to compute
        if moves_checked == 0 {
            if in_check {
                return -MATE + self.position.ply as Eval;
            } else {
                return 0;
            }
        };

        if !self.stop {
            let tt_entry = TTField::new(&self.position, tt_bound, best_move, alpha, depth);
            self.tt.insert(tt_entry);
        }

        alpha
    }

    /// Quiescence search (only look at capture moves)
    fn quiescence(&mut self, mut alpha: Eval, beta: Eval) -> Eval {
        if self.stop || !self.clock.mid_check(self.nodes) {
            self.stop = true;
            return 0;
        }

        self.nodes += 1;
        self.seldepth = max(self.seldepth, self.position.ply);

        // try stand pat
        let eval = self.position.evaluate();

        if self.position.ply >= MAX_DEPTH {
            return eval;
        }

        if eval >= beta {
            return beta;
        }
        if eval < alpha - FUTILITY_MARGIN {
            return alpha; // futility pruning
        }
        alpha = max(eval, alpha); // stand pat is pv

        for (m, s) in self.position.generate_captures() {
            if s < 0 {
                break; // we reached negative see, it's probably not worth searching
            }

            self.position.make_move(m);
            let eval = -self.quiescence(-beta, -alpha);
            self.position.undo_move();

            if eval > alpha {
                if eval >= beta {
                    return beta;
                }
                alpha = eval;
            }
        }

        alpha
    }

    /// Recover pv by traversing the tt from the root
    fn recover_pv(&self, depth: usize) -> Vec<Move> {
        let mut board = self.position.board.clone();
        let mut pv: Vec<Move> = Vec::new();

        for _ in 0..depth {
            let tt_move = match self.tt.probe(board.hash) {
                Some(e) => e.get_move(),
                None => break,
            };

            // move "sanity" check, since a hash collision is possible
            let move_list = board.generate_moves();

            if move_list.moves.contains(&tt_move) {
                board = board.make_move(tt_move);
                pv.push(tt_move);
            } else {
                break;
            }
        }
        pv
    }

    /// Print UCI score info
    fn print_info(&self, eval: Eval, depth: usize) {
        let score = if is_mate(eval) {
            format!("mate {} ", (MATE - eval + 1) / 2)
        } else if is_mated(eval) {
            format!("mate {} ", -(eval + MATE) / 2)
        } else {
            format!("cp {} ", eval)
        };

        let time = max(self.clock.elapsed().as_millis(), 1);

        print!(
            "info time {} score {} depth {} seldepth {} nodes {} nps {} pv ",
            time,
            score,
            depth,
            self.seldepth,
            self.nodes,
            (self.nodes as u128 * 1000) / time,
        );

        let pv = self.recover_pv(depth);
        for m in &pv {
            print!("{} ", m);
        }
        println!();
    }
}

/// Test nodes searched
/// Run with: cargo test --release search -- --show-output
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Instant;

    use crate::{clock::*, piece::Color, position::*, tables::init_all_tables};

    fn search_driver(fen: &str, depth: usize) {
        init_all_tables();
        let position: Position = fen.parse().unwrap();
        let tt = TT::default();
        let clock = Clock::new(
            TimeControl::FixedDepth(depth),
            Arc::new(AtomicBool::new(false)),
            position.board.side == Color::White,
        );

        println!("\n{}\n\n", position.board);

        let mut search: Search = Search::new(position, clock, &tt);
        let start = Instant::now();
        let (best_move, _) = search.iterative_search(true);
        let duration = start.elapsed();

        println!(
            "\nDEPTH: {} Found {} in {:?}\n--------------------------------\n",
            depth, best_move, duration
        );
    }

    #[test]
    fn search_kiwipete10() {
        search_driver(
            "fen r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            10,
        );
    }

    #[test]
    fn search_killer10() {
        search_driver(
            "fen rnbqkb1r/pp1p1pPp/8/2p1pP2/1P1P4/3P3P/P1P1P3/RNBQKBNR w KQkq e6 0 1",
            10,
        );
    }

    #[test]
    fn search_mate4() {
        search_driver("fen 8/8/8/2K5/5Q2/8/4k3/8 w - - 0 1", 20);
    }
}

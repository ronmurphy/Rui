//! 2048 game logic wired to the GTK4 .ui layout.

use gtk4::prelude::*;
use gtk4::{glib, CssProvider, EventControllerKey, Label};
use std::cell::RefCell;
use std::rc::Rc;

// ── Game logic ────────────────────────────────────────────────────────────────

type Board = [[u32; 4]; 4];

struct GameState {
    board: Board,
    score: u32,
    best:  u32,
    moves: u32,
    over:  bool,
    won:   bool,
}

impl GameState {
    fn new() -> Self {
        let mut s = GameState {
            board: [[0; 4]; 4],
            score: 0,
            best:  0,
            moves: 0,
            over:  false,
            won:   false,
        };
        s.spawn();
        s.spawn();
        s
    }

    fn reset(&mut self) {
        let best = self.best;
        *self = GameState::new();
        self.best = best;
    }

    /// Add a 2 (90%) or 4 (10%) to a random empty cell.
    fn spawn(&mut self) -> bool {
        let empties: Vec<(usize, usize)> = (0..4)
            .flat_map(|r| (0..4).map(move |c| (r, c)))
            .filter(|&(r, c)| self.board[r][c] == 0)
            .collect();
        if empties.is_empty() { return false; }
        let idx = (glib::random_int_range(0, empties.len() as i32)) as usize;
        let (r, c) = empties[idx];
        self.board[r][c] = if glib::random_int_range(0, 10) < 1 { 4 } else { 2 };
        true
    }

    /// Slide and merge one row to the left.  Returns (new_row, points).
    fn slide_row(row: [u32; 4]) -> ([u32; 4], u32) {
        // Compact non-zeros
        let vals: Vec<u32> = row.iter().copied().filter(|&v| v != 0).collect();
        let mut out = [0u32; 4];
        let mut pts = 0u32;
        let mut i = 0;
        let mut k = 0;
        while i < vals.len() {
            if i + 1 < vals.len() && vals[i] == vals[i + 1] {
                out[k] = vals[i] * 2;
                pts += out[k];
                i += 2;
            } else {
                out[k] = vals[i];
                i += 1;
            }
            k += 1;
        }
        (out, pts)
    }

    /// Apply a move.  Returns true if the board changed.
    fn apply(&mut self, dir: Direction) -> bool {
        let old = self.board;
        let mut pts = 0u32;

        match dir {
            Direction::Left => {
                for r in 0..4 {
                    let (row, p) = Self::slide_row(self.board[r]);
                    self.board[r] = row; pts += p;
                }
            }
            Direction::Right => {
                for r in 0..4 {
                    let mut rev = self.board[r]; rev.reverse();
                    let (mut row, p) = Self::slide_row(rev);
                    row.reverse();
                    self.board[r] = row; pts += p;
                }
            }
            Direction::Up => {
                for c in 0..4 {
                    let col = [self.board[0][c], self.board[1][c], self.board[2][c], self.board[3][c]];
                    let (out, p) = Self::slide_row(col);
                    for r in 0..4 { self.board[r][c] = out[r]; }
                    pts += p;
                }
            }
            Direction::Down => {
                for c in 0..4 {
                    let mut col = [self.board[0][c], self.board[1][c], self.board[2][c], self.board[3][c]];
                    col.reverse();
                    let (mut out, p) = Self::slide_row(col);
                    out.reverse();
                    for r in 0..4 { self.board[r][c] = out[r]; }
                    pts += p;
                }
            }
        }

        let changed = self.board != old;
        if changed {
            self.score += pts;
            if self.score > self.best { self.best = self.score; }
            self.moves += 1;
            if !self.won && self.board.iter().any(|row| row.iter().any(|&v| v >= 2048)) {
                self.won = true;
            }
            self.spawn();
            if !self.can_move() { self.over = true; }
        }
        changed
    }

    fn can_move(&self) -> bool {
        for r in 0..4 {
            for c in 0..4 {
                if self.board[r][c] == 0 { return true; }
                if c + 1 < 4 && self.board[r][c] == self.board[r][c + 1] { return true; }
                if r + 1 < 4 && self.board[r][c] == self.board[r + 1][c] { return true; }
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
enum Direction { Left, Right, Up, Down }

// ── Tile CSS ──────────────────────────────────────────────────────────────────

fn tile_css() -> String {
    let colors: &[(u32, &str, &str)] = &[
        (0,    "#cdc1b4", "#776e65"),
        (2,    "#eee4da", "#776e65"),
        (4,    "#ede0c8", "#776e65"),
        (8,    "#f2b179", "#f9f6f2"),
        (16,   "#f59563", "#f9f6f2"),
        (32,   "#f67c5f", "#f9f6f2"),
        (64,   "#f65e3b", "#f9f6f2"),
        (128,  "#edcf72", "#f9f6f2"),
        (256,  "#edcc61", "#f9f6f2"),
        (512,  "#edc850", "#f9f6f2"),
        (1024, "#edc53f", "#f9f6f2"),
        (2048, "#edc22e", "#f9f6f2"),
    ];
    let mut css = String::from(
        "label.tile { border-radius:6px; min-width:80px; min-height:80px; font-weight:bold; font-size:28px; }\n"
    );
    for (val, bg, fg) in colors {
        css.push_str(&format!(
            "label.tile-{} {{ background-color:{}; color:{}; }}\n", val, bg, fg
        ));
    }
    // anything >= 2048 just reuses 2048 color
    css
}

fn tile_class(v: u32) -> &'static str {
    match v {
        0    => "tile-0",
        2    => "tile-2",
        4    => "tile-4",
        8    => "tile-8",
        16   => "tile-16",
        32   => "tile-32",
        64   => "tile-64",
        128  => "tile-128",
        256  => "tile-256",
        512  => "tile-512",
        1024 => "tile-1024",
        _    => "tile-2048",
    }
}

// ── UI wiring ─────────────────────────────────────────────────────────────────

pub fn connect_handlers(builder: &gtk4::Builder, window: &gtk4::ApplicationWindow) {
    // Load tile CSS
    let css = CssProvider::new();
    css.load_from_data(&tile_css());
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Get widgets from builder
    let grid         = builder.object::<gtk4::Grid>("grid").expect("grid");
    let score_lbl    = builder.object::<Label>("current_score").expect("current_score");
    let best_lbl     = builder.object::<Label>("best_score").expect("best_score");
    let moves_lbl    = builder.object::<Label>("moves_count").expect("moves_count");
    let new_btn      = builder.object::<gtk4::Button>("new_game_btn").expect("new_game_btn");
    let over_overlay = builder.object::<gtk4::Widget>("game_over_overlay");
    let over_title   = builder.object::<Label>("game_over_title");
    let final_lbl    = builder.object::<Label>("final_score_label");
    let restart_btn  = builder.object::<gtk4::Button>("restart_from_overlay_btn");

    // Create 16 tile Labels and populate the grid
    let mut tiles: Vec<Vec<Label>> = Vec::new();
    for r in 0..4i32 {
        let mut row_tiles = Vec::new();
        for c in 0..4i32 {
            let lbl = Label::new(Some(""));
            lbl.set_hexpand(true);
            lbl.set_vexpand(true);
            lbl.set_halign(gtk4::Align::Fill);
            lbl.set_valign(gtk4::Align::Fill);
            lbl.add_css_class("tile");
            lbl.add_css_class("tile-0");
            lbl.set_margin_start(6);
            lbl.set_margin_end(6);
            lbl.set_margin_top(6);
            lbl.set_margin_bottom(6);
            grid.attach(&lbl, c, r, 1, 1);
            row_tiles.push(lbl);
        }
        tiles.push(row_tiles);
    }

    let state = Rc::new(RefCell::new(GameState::new()));

    // Helper: refresh all tile Labels and score/move counters from state
    let refresh = {
        let state    = state.clone();
        let tiles    = tiles.clone();
        let score_lbl = score_lbl.clone();
        let best_lbl  = best_lbl.clone();
        let moves_lbl = moves_lbl.clone();
        let over_overlay = over_overlay.clone();
        let over_title   = over_title.clone();
        let final_lbl    = final_lbl.clone();
        move || {
            let gs = state.borrow();
            for r in 0..4 {
                for c in 0..4 {
                    let v = gs.board[r][c];
                    let lbl = &tiles[r][c];
                    // Remove old tile-* classes
                    let ctx_classes = lbl.css_classes();
                    for cls in ctx_classes {
                        let s = cls.as_str();
                        if s.starts_with("tile-") { lbl.remove_css_class(s); }
                    }
                    lbl.add_css_class(tile_class(v));
                    let text = if v == 0 { String::new() } else { v.to_string() };
                    lbl.set_label(&text);
                }
            }
            score_lbl.set_label(&gs.score.to_string());
            best_lbl.set_label(&gs.best.to_string());
            moves_lbl.set_label(&gs.moves.to_string());

            if let Some(ref ov) = over_overlay {
                let show = gs.over || gs.won;
                ov.set_visible(show);
                if show {
                    if let Some(ref t) = over_title {
                        t.set_label(if gs.won { "You Win! 🎉" } else { "Game Over!" });
                    }
                    if let Some(ref fl) = final_lbl {
                        fl.set_label(&format!("Score: {}  |  Best: {}", gs.score, gs.best));
                    }
                }
            }
        }
    };

    // Initial render
    refresh();

    // Keyboard input
    {
        let state   = state.clone();
        let refresh = refresh.clone();
        let kc = EventControllerKey::new();
        kc.connect_key_pressed(move |_, key, _, _| {
            use gtk4::gdk::Key;
            let dir = match key {
                Key::Left  | Key::a | Key::A => Some(Direction::Left),
                Key::Right | Key::d | Key::D => Some(Direction::Right),
                Key::Up    | Key::w | Key::W => Some(Direction::Up),
                Key::Down  | Key::s | Key::S => Some(Direction::Down),
                _ => None,
            };
            if let Some(d) = dir {
                if !state.borrow().over && !state.borrow().won {
                    state.borrow_mut().apply(d);
                    refresh();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(kc);
    }

    // New Game button
    {
        let state   = state.clone();
        let refresh = refresh.clone();
        new_btn.connect_clicked(move |_| {
            state.borrow_mut().reset();
            refresh();
        });
    }

    // Restart from overlay button
    if let Some(btn) = restart_btn {
        let state   = state.clone();
        let refresh = refresh.clone();
        btn.connect_clicked(move |_| {
            state.borrow_mut().reset();
            refresh();
        });
    }
}

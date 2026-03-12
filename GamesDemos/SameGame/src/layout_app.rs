//! SameGame — GTK4 grid match game
//!
//! Click connected groups of 3+ matching emoji tiles to clear them.
//! Tiles fall with gravity.  Score = (n−2)² per clear (classic SameGame).
//! Game ends when no group of 3+ tiles remains anywhere on the board.

#![allow(unused_imports)]
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider,
    EventControllerMotion, Grid, Label,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

// ── Constants ─────────────────────────────────────────────────────────────────

const COLS: usize = 10;
const ROWS: usize = 10;
const MIN_GROUP: usize = 3;
const N_TYPES: usize = 5;

const EMOJI: [&str; N_TYPES] = ["🍎", "🍊", "🍋", "🍇", "🫐"];
const T_CLS:  [&str; N_TYPES] = ["t0", "t1", "t2", "t3", "t4"];

fn score_for(n: usize) -> u32 {
    (n.saturating_sub(2) as u32).pow(2)
}

// ── XorShift RNG (no extra crate needed) ─────────────────────────────────────

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xdeadbeef_cafebabe);
        Rng(seed ^ 0x9e3779b97f4a7c15)
    }
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as usize
    }
}

// ── Game logic ────────────────────────────────────────────────────────────────

struct Game {
    grid:      [[Option<usize>; COLS]; ROWS],
    score:     u32,
    game_over: bool,
}

impl Game {
    fn new() -> Self {
        let mut g = Game {
            grid: [[None; COLS]; ROWS],
            score: 0,
            game_over: false,
        };
        let mut rng = Rng::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                g.grid[row][col] = Some(rng.next() % N_TYPES);
            }
        }
        g
    }

    /// Flood-fill: collect all orthogonally connected tiles of the same type.
    fn flood(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let kind = match self.grid[row][col] { Some(k) => k, None => return vec![] };
        let mut vis = [[false; COLS]; ROWS];
        let mut out = Vec::new();
        let mut stk = vec![(row, col)];
        while let Some((r, c)) = stk.pop() {
            if vis[r][c] || self.grid[r][c] != Some(kind) { continue; }
            vis[r][c] = true;
            out.push((r, c));
            if r > 0        { stk.push((r - 1, c)); }
            if r + 1 < ROWS { stk.push((r + 1, c)); }
            if c > 0        { stk.push((r, c - 1)); }
            if c + 1 < COLS { stk.push((r, c + 1)); }
        }
        out
    }

    /// Try to clear the group at (row, col).  Returns (cleared, pts) or None if too small.
    fn click(&mut self, row: usize, col: usize) -> Option<(usize, u32)> {
        if self.game_over { return None; }
        let group = self.flood(row, col);
        if group.len() < MIN_GROUP { return None; }
        for &(r, c) in &group { self.grid[r][c] = None; }
        self.gravity();
        let pts = score_for(group.len());
        self.score += pts;
        if !self.has_move() { self.game_over = true; }
        Some((group.len(), pts))
    }

    /// Tiles fall downward to fill gaps in each column.
    fn gravity(&mut self) {
        for col in 0..COLS {
            let kept: Vec<usize> = (0..ROWS).filter_map(|r| self.grid[r][col]).collect();
            let empty = ROWS - kept.len();
            for row in 0..ROWS {
                self.grid[row][col] =
                    if row < empty { None } else { Some(kept[row - empty]) };
            }
        }
    }

    fn has_move(&self) -> bool {
        (0..ROWS).any(|r| (0..COLS).any(|c| self.flood(r, c).len() >= MIN_GROUP))
    }
}

// ── UI entry point ────────────────────────────────────────────────────────────

pub fn build_game(app: &Application) {
    install_css();

    let builder     = gtk4::Builder::from_string(include_str!("../layout.ui"));
    let win:        ApplicationWindow = builder.object("main_window").unwrap();
    let score_lbl:  Label             = builder.object("score_label").unwrap();
    let status_lbl: Label             = builder.object("status_label").unwrap();
    let new_btn:    Button            = builder.object("new_game_button").unwrap();
    let grid_box:   GtkBox            = builder.object("grid_container").unwrap();

    win.set_application(Some(app));

    // Build the 10×10 tile grid widget
    let (tile_grid, buttons) = make_tile_grid();
    grid_box.append(&tile_grid);

    let state: Rc<RefCell<Game>>             = Rc::new(RefCell::new(Game::new()));
    let btns:  Rc<RefCell<Vec<Vec<Button>>>> = Rc::new(RefCell::new(buttons));

    sync_grid(&state.borrow(), &btns.borrow());

    // Wire hover + click for every tile
    for row in 0..ROWS {
        for col in 0..COLS {
            let btn = btns.borrow()[row][col].clone();
            wire_tile(&btn, row, col, &state, &btns, &score_lbl, &status_lbl);
        }
    }

    // New Game
    {
        let (s, b, sl, stl) = (
            state.clone(), btns.clone(), score_lbl.clone(), status_lbl.clone(),
        );
        new_btn.connect_clicked(move |_| {
            *s.borrow_mut() = Game::new();
            sync_grid(&s.borrow(), &b.borrow());
            sl.set_label("Score: 0");
            stl.set_label("Click a group of 3+ matching tiles!");
        });
    }

    win.present();
}

// ── Grid construction ─────────────────────────────────────────────────────────

fn make_tile_grid() -> (Grid, Vec<Vec<Button>>) {
    let grid = Grid::new();
    grid.set_row_spacing(3);
    grid.set_column_spacing(3);
    grid.set_halign(gtk4::Align::Center);
    grid.set_margin_start(12);
    grid.set_margin_end(12);
    grid.set_margin_bottom(16);
    grid.add_css_class("game-grid");

    let mut buttons = Vec::new();
    for row in 0..ROWS {
        let mut row_btns = Vec::new();
        for col in 0..COLS {
            let btn = Button::new();
            // Custom CSS element name avoids Adwaita's `button { }` overrides entirely
            btn.add_css_class("tile");
            btn.set_size_request(50, 50);
            grid.attach(&btn, col as i32, row as i32, 1, 1);
            row_btns.push(btn);
        }
        buttons.push(row_btns);
    }
    (grid, buttons)
}

// ── Grid / tile helpers ───────────────────────────────────────────────────────

fn sync_grid(game: &Game, btns: &[Vec<Button>]) {
    for row in 0..ROWS {
        for col in 0..COLS {
            set_tile(&btns[row][col], game.grid[row][col]);
        }
    }
}

fn set_tile(btn: &Button, tile: Option<usize>) {
    for cls in T_CLS      { btn.remove_css_class(cls); }
    btn.remove_css_class("tile-glow");
    btn.remove_css_class("tile-dim");
    btn.remove_css_class("empty");
    match tile {
        Some(k) => { btn.set_label(EMOJI[k]); btn.add_css_class(T_CLS[k]); }
        None    => { btn.set_label("");        btn.add_css_class("empty");  }
    }
}

fn clear_highlights(btns: &[Vec<Button>]) {
    for row in btns {
        for btn in row {
            btn.remove_css_class("tile-glow");
            btn.remove_css_class("tile-dim");
        }
    }
}

// ── Signal wiring ─────────────────────────────────────────────────────────────

fn wire_tile(
    btn:        &Button,
    row:        usize,
    col:        usize,
    state:      &Rc<RefCell<Game>>,
    btns:       &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl:  &Label,
    status_lbl: &Label,
) {
    // ── Hover: show which group would be cleared ──────────────────────────────
    let motion = EventControllerMotion::new();
    {
        let (s, b) = (state.clone(), btns.clone());
        motion.connect_enter(move |_, _, _| {
            let game = s.borrow();
            if game.game_over { return; }
            let group = game.flood(row, col);
            let valid = group.len() >= MIN_GROUP;
            let btns_ref = b.borrow();
            clear_highlights(&btns_ref);
            if valid {
                let set: HashSet<(usize, usize)> = group.into_iter().collect();
                for rr in 0..ROWS {
                    for cc in 0..COLS {
                        if set.contains(&(rr, cc)) {
                            btns_ref[rr][cc].add_css_class("tile-glow");
                        } else if game.grid[rr][cc].is_some() {
                            btns_ref[rr][cc].add_css_class("tile-dim");
                        }
                    }
                }
            }
        });
    }
    {
        let b = btns.clone();
        motion.connect_leave(move |_| { clear_highlights(&b.borrow()); });
    }
    btn.add_controller(motion);

    // ── Click: clear the group ────────────────────────────────────────────────
    {
        let (s, b, sl, stl) = (
            state.clone(), btns.clone(), score_lbl.clone(), status_lbl.clone(),
        );
        btn.connect_clicked(move |_| {
            let result = s.borrow_mut().click(row, col);
            match result {
                None => {
                    if !s.borrow().game_over {
                        stl.set_label("⚠️  Need 3+ connected matching tiles!");
                    }
                }
                Some((n, pts)) => {
                    let game = s.borrow();
                    sl.set_label(&format!("Score: {}", game.score));
                    if game.game_over {
                        stl.set_label(&format!(
                            "🏁 No more moves!  Final score: {}   (click New Game to play again)",
                            game.score
                        ));
                    } else {
                        stl.set_label(&format!("✨ Removed {} tiles  +{} pts", n, pts));
                    }
                    sync_grid(&game, &b.borrow());
                }
            }
        });
    }
}

// ── CSS ───────────────────────────────────────────────────────────────────────

fn install_css() {
    let css = CssProvider::new();
    css.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

const STYLE: &str = r#"
window { background-color: #1a1a2e; }

#header_box {
    background-color: #16213e;
    padding: 10px 16px;
    border-bottom: 1px solid #0a0a1a;
}
#title_label {
    font-size: 21px;
    font-weight: bold;
    color: #e8e8ff;
}
#score_label {
    font-size: 16px;
    font-weight: bold;
    color: #ffd700;
    padding: 3px 14px;
    background-color: #0f1a2e;
    border-radius: 8px;
}
#new_game_button {
    background-color: #e94560;
    color: white;
    font-weight: bold;
    border-radius: 8px;
    padding: 5px 16px;
    border: none;
}
#new_game_button:hover { background-color: #ff6b87; }

#status_label {
    color: #8888aa;
    font-size: 12px;
    padding: 5px 16px 6px 16px;
}

.game-grid {
    background-color: #0d0d1a;
    padding: 10px;
    border-radius: 12px;
    border: 1px solid #2a2a4a;
}

/* "tile" is our custom CSS element name — avoids fighting Adwaita's button styles */
tile {
    border-radius: 8px;
    border: 2px solid transparent;
    min-width: 50px;
    min-height: 50px;
    padding: 0;
    transition: border-color 80ms, opacity 80ms;
}
tile label { font-size: 24px; }

tile.t0 { background-color: #c0392b; color: white; }
tile.t1 { background-color: #d35400; color: white; }
tile.t2 { background-color: #c9a800; color: #222;  }
tile.t3 { background-color: #7d3c98; color: white; }
tile.t4 { background-color: #1a5276; color: white; }

tile.t0:hover { background-color: #e74c3c; }
tile.t1:hover { background-color: #e67e22; }
tile.t2:hover { background-color: #f1c40f; color: #222; }
tile.t3:hover { background-color: #9b59b6; }
tile.t4:hover { background-color: #2980b9; }

tile.empty {
    background-color: #0a0a16;
    border-color: #1a1a30;
    opacity: 0.4;
}

/* Glow: brighter tile + white border for the hovered group.
   Defined after t0-t4 so same-specificity border rule wins. */
tile.t0.tile-glow { background-color: #e74c3c; border-color: rgba(255,255,255,0.85); }
tile.t1.tile-glow { background-color: #e67e22; border-color: rgba(255,255,255,0.85); }
tile.t2.tile-glow { background-color: #f1c40f; border-color: rgba(255,255,255,0.85); color: #222; }
tile.t3.tile-glow { background-color: #9b59b6; border-color: rgba(255,255,255,0.85); }
tile.t4.tile-glow { background-color: #2980b9; border-color: rgba(255,255,255,0.85); }

tile.tile-dim { opacity: 0.22; }
"#;
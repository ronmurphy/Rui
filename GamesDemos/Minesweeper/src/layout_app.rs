//! Minesweeper game logic for layout.ui
use gtk4::prelude::*;
use gtk4::{glib, Builder, Button, CssProvider, GestureClick, Grid, Label};
use std::cell::RefCell;
use std::rc::Rc;

const ROWS: usize = 9;
const COLS: usize = 9;
const MINES: usize = 10;

static ADJ_LABEL: [&str; 9] = ["", "1", "2", "3", "4", "5", "6", "7", "8"];

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Cell {
    is_mine: bool,
    is_revealed: bool,
    is_flagged: bool,
    adjacent: u8,
}

impl Cell {
    fn new() -> Self {
        Cell {
            is_mine: false,
            is_revealed: false,
            is_flagged: false,
            adjacent: 0,
        }
    }
}

struct GameState {
    cells: Vec<Vec<Cell>>,
    game_over: bool,
    won: bool,
    first_click: bool,
    mines_remaining: i32,
}

impl GameState {
    fn new() -> Self {
        GameState {
            cells: vec![vec![Cell::new(); COLS]; ROWS],
            game_over: false,
            won: false,
            first_click: true,
            mines_remaining: MINES as i32,
        }
    }

    /// Place mines randomly, guaranteeing (safe_r, safe_c) is clear.
    fn place_mines(&mut self, safe_r: usize, safe_c: usize) {
        let mut placed = 0;
        while placed < MINES {
            let n = glib::random_int_range(0, (ROWS * COLS) as i32) as usize;
            let (r, c) = (n / COLS, n % COLS);
            if !self.cells[r][c].is_mine && !(r == safe_r && c == safe_c) {
                self.cells[r][c].is_mine = true;
                placed += 1;
            }
        }
        for r in 0..ROWS {
            for c in 0..COLS {
                if !self.cells[r][c].is_mine {
                    self.cells[r][c].adjacent = self.count_adj(r, c);
                }
            }
        }
    }

    fn count_adj(&self, row: usize, col: usize) -> u8 {
        let mut n = 0u8;
        for dr in -1i32..=1 {
            for dc in -1i32..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let (r, c) = (row as i32 + dr, col as i32 + dc);
                if r >= 0
                    && r < ROWS as i32
                    && c >= 0
                    && c < COLS as i32
                    && self.cells[r as usize][c as usize].is_mine
                {
                    n += 1;
                }
            }
        }
        n
    }

    /// Reveal a cell; flood-fills when adjacent == 0. Returns true if mine hit.
    fn reveal(&mut self, row: usize, col: usize) -> bool {
        if self.cells[row][col].is_flagged || self.cells[row][col].is_revealed {
            return false;
        }
        self.cells[row][col].is_revealed = true;
        if self.cells[row][col].is_mine {
            return true;
        }
        if self.cells[row][col].adjacent == 0 {
            for dr in -1i32..=1 {
                for dc in -1i32..=1 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let (r, c) = (row as i32 + dr, col as i32 + dc);
                    if r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
                        self.reveal(r as usize, c as usize);
                    }
                }
            }
        }
        false
    }

    fn check_win(&self) -> bool {
        self.cells
            .iter()
            .flatten()
            .all(|c| c.is_mine || c.is_revealed)
    }
}

// ── Button appearance helper ──────────────────────────────────────────────────

fn sync_button(btn: &Button, cell: &Cell, game_over: bool) {
    btn.remove_css_class("revealed");
    btn.remove_css_class("mine");
    btn.remove_css_class("flagged");

    if cell.is_revealed {
        if cell.is_mine {
            btn.set_label("💣");
            btn.add_css_class("mine");
        } else {
            btn.set_label(ADJ_LABEL[cell.adjacent as usize]);
            btn.add_css_class("revealed");
            btn.set_sensitive(false);
        }
    } else if cell.is_flagged {
        btn.set_label("🚩");
        btn.add_css_class("flagged");
    } else if game_over && cell.is_mine {
        // Expose undetonated mines on loss
        btn.set_label("💣");
        btn.add_css_class("mine");
        btn.set_sensitive(false);
    } else {
        btn.set_label("");
        btn.set_sensitive(!game_over);
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

#[allow(unused_imports)]
use gtk4::prelude::*;

pub fn connect_handlers(builder: &Builder) {
    let game_grid: Grid = builder.object("game_grid").unwrap();
    let mine_lbl: Label = builder.object("mine_count_label").unwrap();
    let timer_lbl: Label = builder.object("timer_label").unwrap();
    let reset_btn: Button = builder.object("reset_button").unwrap();

    // ── CSS ───────────────────────────────────────────────────────────────
    let css = CssProvider::new();
    css.load_from_data(
        "button          { min-width:36px; min-height:36px; padding:0; font-weight:bold; }
         button.revealed { background:#c0c0c0; box-shadow:none; border:1px solid #999; }
         button.mine     { background:#e74c3c; color:white; }
         button.flagged  { color:#c0392b; }",
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // ── Shared state ──────────────────────────────────────────────────────
    let state: Rc<RefCell<GameState>> = Rc::new(RefCell::new(GameState::new()));
    let timer_active: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let elapsed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    // ── Build cell buttons ────────────────────────────────────────────────
    let mut btns_vec: Vec<Vec<Button>> = Vec::with_capacity(ROWS);
    for r in 0..ROWS {
        let mut row = Vec::with_capacity(COLS);
        for c in 0..COLS {
            let btn = Button::builder().label("").build();
            game_grid.attach(&btn, c as i32, r as i32, 1, 1);
            row.push(btn);
        }
        btns_vec.push(row);
    }
    let buttons: Rc<RefCell<Vec<Vec<Button>>>> = Rc::new(RefCell::new(btns_vec));

    // ── 1-second timer tick ───────────────────────────────────────────────
    {
        let timer_active = timer_active.clone();
        let elapsed = elapsed.clone();
        let timer_lbl = timer_lbl.clone();
        glib::timeout_add_seconds_local(1, move || {
            if *timer_active.borrow() {
                let mut e = elapsed.borrow_mut();
                *e = (*e + 1).min(999);
                timer_lbl.set_label(&format!("⏱ {:03}", *e));
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Reset button ──────────────────────────────────────────────────────
    {
        let state = state.clone();
        let buttons = buttons.clone();
        let mine_lbl = mine_lbl.clone();
        let timer_lbl = timer_lbl.clone();
        let timer_active = timer_active.clone();
        let elapsed = elapsed.clone();
        reset_btn.connect_clicked(move |btn| {
            *state.borrow_mut() = GameState::new();
            *timer_active.borrow_mut() = false;
            *elapsed.borrow_mut() = 0;
            btn.set_label("🙂");
            mine_lbl.set_label(&format!("💣 {:03}", MINES));
            timer_lbl.set_label("⏱ 000");
            let btns = buttons.borrow();
            for r in 0..ROWS {
                for c in 0..COLS {
                    let b = &btns[r][c];
                    b.set_label("");
                    b.set_sensitive(true);
                    b.remove_css_class("revealed");
                    b.remove_css_class("mine");
                    b.remove_css_class("flagged");
                }
            }
        });
    }

    // ── Cell signal handlers ──────────────────────────────────────────────
    for r in 0..ROWS {
        for c in 0..COLS {
            let btn = buttons.borrow()[r][c].clone();

            // Left-click: reveal
            {
                let state = state.clone();
                let buttons = buttons.clone();
                let timer_active = timer_active.clone();
                let reset_btn = reset_btn.clone();
                btn.connect_clicked(move |_| {
                    // Mutate state, collect outcome, then drop borrow
                    let (hit, won) = {
                        let mut st = state.borrow_mut();
                        if st.game_over || st.won {
                            return;
                        }
                        if st.cells[r][c].is_revealed || st.cells[r][c].is_flagged {
                            return;
                        }
                        if st.first_click {
                            st.first_click = false;
                            st.place_mines(r, c);
                            *timer_active.borrow_mut() = true;
                        }
                        let hit = st.reveal(r, c);
                        if hit {
                            st.game_over = true;
                        }
                        let won = !hit && st.check_win();
                        if won {
                            st.won = true;
                        }
                        (hit, won)
                    };
                    // Sync all buttons (read-only borrow now safe)
                    {
                        let st = state.borrow();
                        let btns = buttons.borrow();
                        for rr in 0..ROWS {
                            for cc in 0..COLS {
                                sync_button(&btns[rr][cc], &st.cells[rr][cc], hit);
                            }
                        }
                    }
                    if hit {
                        reset_btn.set_label("😵");
                        *timer_active.borrow_mut() = false;
                    } else if won {
                        reset_btn.set_label("😎");
                        *timer_active.borrow_mut() = false;
                    }
                });
            }

            // Right-click: toggle flag
            {
                let state = state.clone();
                let buttons = buttons.clone();
                let mine_lbl = mine_lbl.clone();
                let gesture = GestureClick::new();
                gesture.set_button(3);
                gesture.connect_released(move |_, _, _, _| {
                    let remaining = {
                        let mut st = state.borrow_mut();
                        if st.game_over || st.won {
                            return;
                        }
                        let cell = &mut st.cells[r][c];
                        if cell.is_revealed {
                            return;
                        }
                        cell.is_flagged = !cell.is_flagged;
                        if cell.is_flagged {
                            st.mines_remaining -= 1;
                        } else {
                            st.mines_remaining += 1;
                        }
                        st.mines_remaining
                    };
                    mine_lbl.set_label(&format!("💣 {:03}", remaining));
                    let st = state.borrow();
                    let btns = buttons.borrow();
                    sync_button(&btns[r][c], &st.cells[r][c], false);
                });
                btn.add_controller(gesture);
            }
        }
    }
}

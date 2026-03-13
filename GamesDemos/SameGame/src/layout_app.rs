//! SameGame — GTK4 grid match game with levels, power-ups, and environmental tiles
#![allow(unused_imports)]
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider,
    EventControllerMotion, GestureClick, Grid, Label, Window,
};
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

// ── Constants ────────────────────────────────────────────────────────────────

const COLS: usize = 10;
const ROWS: usize = 10;
const MIN_GROUP: usize = 3;
const N_FRUITS: usize = 5;

const FRUIT_EMOJI: [&str; N_FRUITS] = ["🍎", "🍊", "🍋", "🍇", "🫐"];
const FRUIT_CLS: [&str; N_FRUITS] = ["t0", "t1", "t2", "t3", "t4"];

fn score_for(n: usize) -> u32 {
    (n.saturating_sub(2) as u32).pow(2)
}

// ── Tile types ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tile {
    Fruit(u8),    // 0..4 normal fruit
    Bomb,         // 💣 clears 3×3 area
    Lightning,    // ⚡ clears entire row
    Rainbow,      // 🌈 clears all of one fruit color
    Frozen(u8),   // 🧊 wrapped fruit — must be adjacent to a clear to thaw
    Wildcard,     // 🃏 matches any adjacent fruit color for flood-fill
    Blocker,      // 🪨 indestructible obstacle (cleared only by bomb/lightning)
}

impl Tile {
    fn emoji(self) -> &'static str {
        match self {
            Tile::Fruit(k) => FRUIT_EMOJI[k as usize],
            Tile::Bomb => "💣",
            Tile::Lightning => "⚡",
            Tile::Rainbow => "🌈",
            Tile::Frozen(k) => FRUIT_EMOJI[k as usize],
            Tile::Wildcard => "🃏",
            Tile::Blocker => "🪨",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            Tile::Fruit(k) => FRUIT_CLS[k as usize],
            Tile::Bomb => "power-bomb",
            Tile::Lightning => "power-lightning",
            Tile::Rainbow => "power-rainbow",
            Tile::Frozen(k) => FRUIT_CLS[k as usize],
            Tile::Wildcard => "power-wildcard",
            Tile::Blocker => "env-blocker",
        }
    }

    /// Is this a normal fruit (or wildcard that acts like one)?
    fn is_matchable(self) -> bool {
        matches!(self, Tile::Fruit(_) | Tile::Wildcard)
    }

    /// fruit index for matching purposes (wildcard returns None — it matches any)
    fn fruit_kind(self) -> Option<u8> {
        match self {
            Tile::Fruit(k) => Some(k),
            _ => None,
        }
    }
}

// ── XorShift RNG ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Rng(u64);
impl Rng {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xdeadbeef_cafebabe);
        Rng(seed ^ 0x9e3779b97f4a7c15)
    }
    fn next_usize(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as usize
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_usize() % 10000) as f64 / 10000.0
    }
    fn rand_fruit(&mut self) -> u8 {
        (self.next_usize() % N_FRUITS) as u8
    }
}

// ── Game state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Game {
    grid: [[Option<Tile>; COLS]; ROWS],
    score: u32,
    level: u32,
    level_score: u32,       // score within current level (resets each level)
    level_target: u32,      // points needed this level
    high_score: u32,
    game_over: bool,
    selected_pos: Option<(usize, usize)>,
    combo: u32,
    rng: Rng,
    /// Pending message from power-up activations
    last_powerup_msg: Option<String>,
}

impl Game {
    fn new() -> Self {
        let mut rng = Rng::new();
        let mut g = Game {
            grid: [[None; COLS]; ROWS],
            score: 0,
            level: 1,
            level_score: 0,
            level_target: 100,
            high_score: 0,
            game_over: false,
            selected_pos: None,
            combo: 0,
            rng: rng.clone(),
            last_powerup_msg: None,
        };
        for row in 0..ROWS {
            for col in 0..COLS {
                g.grid[row][col] = Some(Tile::Fruit(rng.rand_fruit()));
            }
        }
        g.rng = rng;
        g
    }

    fn new_keeping_hiscore(hi: u32) -> Self {
        let mut g = Self::new();
        g.high_score = hi;
        g
    }

    /// Flood-fill for matching fruit. Wildcards match any adjacent fruit color.
    fn flood(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let tile = match self.grid[row][col] {
            Some(t) if t.is_matchable() => t,
            _ => return vec![],
        };
        // Determine the target fruit kind.
        // If starting on a wildcard, look at neighbors to pick a color.
        let kind = match tile.fruit_kind() {
            Some(k) => k,
            None => {
                // Wildcard: find an adjacent fruit to adopt its color
                let neighbors = self.cardinal_neighbors(row, col);
                match neighbors.iter().find_map(|&(r, c)| {
                    self.grid[r][c].and_then(|t| t.fruit_kind())
                }) {
                    Some(k) => k,
                    None => return vec![(row, col)], // isolated wildcard
                }
            }
        };

        let mut vis = [[false; COLS]; ROWS];
        let mut out = Vec::new();
        let mut stk = vec![(row, col)];
        while let Some((r, c)) = stk.pop() {
            if vis[r][c] {
                continue;
            }
            match self.grid[r][c] {
                Some(Tile::Fruit(k)) if k == kind => {}
                Some(Tile::Wildcard) => {}
                _ => continue,
            }
            vis[r][c] = true;
            out.push((r, c));
            for (nr, nc) in self.cardinal_neighbors(r, c) {
                stk.push((nr, nc));
            }
        }
        out
    }

    fn cardinal_neighbors(&self, r: usize, c: usize) -> Vec<(usize, usize)> {
        let mut v = Vec::with_capacity(4);
        if r > 0 { v.push((r - 1, c)); }
        if r + 1 < ROWS { v.push((r + 1, c)); }
        if c > 0 { v.push((r, c - 1)); }
        if c + 1 < COLS { v.push((r, c + 1)); }
        v
    }

    /// Handle a left-click. Returns Some((tiles_removed, points, leveled_up))
    /// or None if the click did nothing.
    fn click(&mut self, row: usize, col: usize) -> Option<(usize, u32, bool)> {
        if self.game_over { return None; }

        // Check if clicking a power-up tile directly
        match self.grid[row][col] {
            Some(Tile::Bomb) => return self.activate_bomb(row, col),
            Some(Tile::Lightning) => return self.activate_lightning(row),
            Some(Tile::Rainbow) => return self.activate_rainbow(),
            _ => {}
        }

        let group = self.flood(row, col);
        if group.len() < MIN_GROUP {
            self.combo = 0;
            return None;
        }

        // Thaw adjacent frozen tiles
        let mut thawed = Vec::new();
        let group_set: HashSet<(usize, usize)> = group.iter().copied().collect();
        for &(r, c) in &group {
            for (nr, nc) in self.cardinal_neighbors(r, c) {
                if !group_set.contains(&(nr, nc)) {
                    if let Some(Tile::Frozen(k)) = self.grid[nr][nc] {
                        self.grid[nr][nc] = Some(Tile::Fruit(k));
                        thawed.push((nr, nc));
                    }
                }
            }
        }

        // Clear matched tiles
        for &(r, c) in &group {
            self.grid[r][c] = None;
        }
        self.gravity();

        self.combo += 1;
        let combo_mult = if self.combo >= 3 {
            1.0 + (self.combo - 2) as f64 * 0.5
        } else {
            1.0
        };
        let base_pts = score_for(group.len());
        let pts = (base_pts as f64 * combo_mult) as u32;
        self.score += pts;
        self.level_score += pts;
        if self.score > self.high_score {
            self.high_score = self.score;
        }

        // Maybe spawn a power-up in a random empty cell
        self.maybe_spawn_powerup(group.len());

        if !thawed.is_empty() {
            self.last_powerup_msg = Some(format!("🧊 Thawed {} frozen tile(s)!", thawed.len()));
        }

        // Check level-up
        let leveled_up = self.level_score >= self.level_target;
        if leveled_up {
            self.level += 1;
            self.level_score = 0;
            self.level_target += 50;
        }

        if !leveled_up && !self.has_move() {
            self.game_over = true;
        }
        Some((group.len(), pts, leveled_up))
    }

    fn activate_bomb(&mut self, row: usize, col: usize) -> Option<(usize, u32, bool)> {
        let mut count = 0;
        for dr in -1i32..=1 {
            for dc in -1i32..=1 {
                let r = row as i32 + dr;
                let c = col as i32 + dc;
                if r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
                    let ru = r as usize;
                    let cu = c as usize;
                    if self.grid[ru][cu].is_some() {
                        self.grid[ru][cu] = None;
                        count += 1;
                    }
                }
            }
        }
        self.gravity();
        let pts = count * 3;
        self.score += pts;
        self.level_score += pts;
        if self.score > self.high_score { self.high_score = self.score; }
        self.last_powerup_msg = Some(format!("💣 BOOM! Cleared {} tiles!", count));
        let leveled_up = self.level_score >= self.level_target;
        if leveled_up {
            self.level += 1;
            self.level_score = 0;
            self.level_target += 50;
        }
        if !leveled_up && !self.has_move() { self.game_over = true; }
        Some((count as usize, pts, leveled_up))
    }

    fn activate_lightning(&mut self, row: usize) -> Option<(usize, u32, bool)> {
        let mut count = 0;
        for c in 0..COLS {
            if self.grid[row][c].is_some() {
                self.grid[row][c] = None;
                count += 1;
            }
        }
        self.gravity();
        let pts = count * 4;
        self.score += pts;
        self.level_score += pts;
        if self.score > self.high_score { self.high_score = self.score; }
        self.last_powerup_msg = Some(format!("⚡ ZAP! Cleared row — {} tiles!", count));
        let leveled_up = self.level_score >= self.level_target;
        if leveled_up {
            self.level += 1;
            self.level_score = 0;
            self.level_target += 50;
        }
        if !leveled_up && !self.has_move() { self.game_over = true; }
        Some((count as usize, pts, leveled_up))
    }

    fn activate_rainbow(&mut self) -> Option<(usize, u32, bool)> {
        // Find the most common fruit color and clear all of them
        let mut counts = [0u32; N_FRUITS];
        for r in 0..ROWS {
            for c in 0..COLS {
                if let Some(Tile::Fruit(k)) = self.grid[r][c] {
                    counts[k as usize] += 1;
                }
            }
        }
        let target = counts.iter().enumerate()
            .max_by_key(|(_, &n)| n)
            .map(|(i, _)| i as u8)
            .unwrap_or(0);

        let mut count = 0;
        for r in 0..ROWS {
            for c in 0..COLS {
                match self.grid[r][c] {
                    Some(Tile::Fruit(k)) if k == target => {
                        self.grid[r][c] = None;
                        count += 1;
                    }
                    Some(Tile::Rainbow) => {
                        self.grid[r][c] = None;
                        count += 1;
                    }
                    _ => {}
                }
            }
        }
        self.gravity();
        let pts = count * 5;
        self.score += pts;
        self.level_score += pts;
        if self.score > self.high_score { self.high_score = self.score; }
        self.last_powerup_msg = Some(format!(
            "🌈 Rainbow! Cleared all {} — {} tiles!",
            FRUIT_EMOJI[target as usize], count
        ));
        let leveled_up = self.level_score >= self.level_target;
        if leveled_up {
            self.level += 1;
            self.level_score = 0;
            self.level_target += 50;
        }
        if !leveled_up && !self.has_move() { self.game_over = true; }
        Some((count as usize, pts, leveled_up))
    }

    fn maybe_spawn_powerup(&mut self, cleared: usize) {
        // Collect empty cells
        let mut empties: Vec<(usize, usize)> = Vec::new();
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.grid[r][c].is_none() {
                    empties.push((r, c));
                }
            }
        }
        if empties.is_empty() { return; }

        let roll = self.rng.next_f64();

        // Bigger clears = higher chance. Combo boosts chance too.
        let bonus = (cleared as f64 - 3.0) * 0.02 + (self.combo as f64) * 0.03;

        let tile = if roll < 0.10 + bonus {
            Some(Tile::Bomb)
        } else if roll < 0.17 + bonus {
            Some(Tile::Lightning)
        } else if roll < 0.21 + bonus {
            Some(Tile::Rainbow)
        } else if roll < 0.26 + bonus {
            Some(Tile::Wildcard)
        } else {
            None
        };

        if let Some(t) = tile {
            let idx = self.rng.next_usize() % empties.len();
            let (r, c) = empties[idx];
            self.grid[r][c] = Some(t);
            // Power-up spawns at bottom due to gravity — re-run gravity
            self.gravity();
        }
    }

    fn gravity(&mut self) {
        for col in 0..COLS {
            let kept: Vec<Tile> = (0..ROWS)
                .filter_map(|r| self.grid[r][col])
                .collect();
            let empty = ROWS - kept.len();
            for row in 0..ROWS {
                self.grid[row][col] = if row < empty {
                    None
                } else {
                    Some(kept[row - empty])
                };
            }
        }
    }

    /// Swap two adjacent tiles. Only allowed if the swap creates a match for
    /// at least one of the two tiles.
    fn swap_tiles(&mut self, r1: usize, c1: usize, r2: usize, c2: usize) -> Result<(), &'static str> {
        let delta_r = (r1 as i32) - (r2 as i32);
        let delta_c = (c1 as i32) - (c2 as i32);
        if (delta_r.abs() + delta_c.abs()) != 1 {
            return Err("Must be adjacent!");
        }
        if self.grid[r1][c1].is_none() || self.grid[r2][c2].is_none() {
            return Err("Can't swap empty!");
        }
        // Don't allow swapping blockers
        if matches!(self.grid[r1][c1], Some(Tile::Blocker))
            || matches!(self.grid[r2][c2], Some(Tile::Blocker))
        {
            return Err("Can't move 🪨!");
        }
        if self.grid[r1][c1] == self.grid[r2][c2] {
            return Err("Same tile type!");
        }

        // Tentatively swap
        let t1 = self.grid[r1][c1];
        let t2 = self.grid[r2][c2];
        self.grid[r1][c1] = t2;
        self.grid[r2][c2] = t1;

        // Check if either position now has a valid group
        let g1 = self.flood(r1, c1);
        let g2 = self.flood(r2, c2);

        // Also allow if swapping INTO a power-up (activating it counts as valid)
        let has_powerup = matches!(t1, Some(Tile::Bomb | Tile::Lightning | Tile::Rainbow))
            || matches!(t2, Some(Tile::Bomb | Tile::Lightning | Tile::Rainbow));

        if g1.len() >= MIN_GROUP || g2.len() >= MIN_GROUP || has_powerup {
            self.gravity();
            Ok(())
        } else {
            // Swap back
            self.grid[r1][c1] = t1;
            self.grid[r2][c2] = t2;
            Err("No match created!")
        }
    }

    fn has_move(&self) -> bool {
        (0..ROWS).any(|r| {
            (0..COLS).any(|c| {
                // Normal flood match
                if self.flood(r, c).len() >= MIN_GROUP {
                    return true;
                }
                // Clickable power-up
                matches!(
                    self.grid[r][c],
                    Some(Tile::Bomb | Tile::Lightning | Tile::Rainbow)
                )
            })
        })
    }

    fn empty_count(&self) -> usize {
        (0..ROWS)
            .flat_map(|r| (0..COLS).map(move |c| (r, c)))
            .filter(|&(r, c)| self.grid[r][c].is_none())
            .count()
    }

    /// Refill empties with fruit + some hazards based on level.
    fn refill_empties(&mut self) {
        let level = self.level;
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.grid[row][col].is_none() {
                    let roll = self.rng.next_f64();
                    // Higher levels → more frozen tiles and occasional blockers
                    let frozen_chance = 0.02 * level as f64;
                    let blocker_chance = if level >= 3 { 0.01 * (level - 2) as f64 } else { 0.0 };

                    if roll < blocker_chance.min(0.05) {
                        self.grid[row][col] = Some(Tile::Blocker);
                    } else if roll < blocker_chance + frozen_chance.min(0.15) {
                        self.grid[row][col] = Some(Tile::Frozen(self.rng.rand_fruit()));
                    } else {
                        self.grid[row][col] = Some(Tile::Fruit(self.rng.rand_fruit()));
                    }
                }
            }
        }
        self.gravity();
    }
}

// ── Main Entry ───────────────────────────────────────────────────────────────

pub fn build_game(app: &Application) {
    install_css();

    let builder = gtk4::Builder::from_string(include_str!("../layout.ui"));
    let win: ApplicationWindow = builder.object("main_window").expect("Window not found");
    let score_lbl: Label = builder.object("score_label").expect("Score label not found");
    let level_lbl: Label = builder.object("level_label").expect("Level label not found");
    let hiscore_lbl: Label = builder.object("hiscore_label").expect("Hiscore label not found");
    let status_lbl: Label = builder.object("status_label").expect("Status label not found");
    let new_btn: Button = builder.object("new_game_button").expect("New game button not found");
    let help_btn: Button = builder.object("help_button").expect("Help button not found");
    let grid_box: GtkBox = builder.object("grid_container").expect("Grid container not found");

    win.set_application(Some(app));

    let (tile_grid, buttons) = make_tile_grid();
    grid_box.append(&tile_grid);

    let state = Rc::new(RefCell::new(Game::new()));
    let btns = Rc::new(RefCell::new(buttons));
    let animating = Rc::new(Cell::new(false));

    sync_grid(&state.borrow(), &btns.borrow());
    update_labels(&score_lbl, &level_lbl, &hiscore_lbl, &state.borrow());

    for row in 0..ROWS {
        for col in 0..COLS {
            let btn = btns.borrow()[row][col].clone();
            wire_tile(
                &btn, row, col, &state, &btns,
                &score_lbl, &level_lbl, &hiscore_lbl, &status_lbl, &animating,
            );
        }
    }

    // New Game button
    {
        let s = state.clone();
        let b = btns.clone();
        let sl = score_lbl.clone();
        let ll = level_lbl.clone();
        let hl = hiscore_lbl.clone();
        let stl = status_lbl.clone();
        let anim = animating.clone();
        new_btn.connect_clicked(move |_| {
            if anim.get() { return; }
            let hi = s.borrow().high_score;
            *s.borrow_mut() = Game::new_keeping_hiscore(hi);
            sync_grid(&s.borrow(), &b.borrow());
            update_labels(&sl, &ll, &hl, &s.borrow());
            stl.set_label("Click a group of 3+ matching tiles!");
        });
    }

    // Help button
    {
        let w = win.clone();
        help_btn.connect_clicked(move |_| show_help(&w));
    }

    win.present();
}

// ── Help dialog ──────────────────────────────────────────────────────────────

fn show_help(parent: &ApplicationWindow) {
    let dialog = Window::builder()
        .title("How to Play — SameGame")
        .default_width(420)
        .default_height(520)
        .transient_for(parent)
        .modal(true)
        .build();

    let vbox = GtkBox::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);

    let sections = [
        ("🎯 Basics", "\
• Click a group of 3+ connected same-color fruit to clear them\n\
• Score = (tiles - 2)² per clear\n\
• Consecutive clears build a combo multiplier (×1.5, ×2, ...)\n\
• Right-click a tile to select it, then right-click an adjacent\n  \
  tile to swap — but only if the swap creates a match!"),
        ("📊 Levels", "\
• Each level has a point target (starts at 100, +50 each level)\n\
• Score resets to 0 each level — reach the target to advance\n\
• On level-up, empty cells fill with new fruit (and hazards!)"),
        ("⚡ Power-Ups", "\
• 💣 Bomb — Click to clear a 3×3 area around it\n\
• ⚡ Lightning — Click to clear the entire row\n\
• 🌈 Rainbow — Click to clear ALL tiles of the most common color\n\
• 🃏 Wildcard — Matches any adjacent fruit color in groups\n\n\
Power-ups spawn randomly after clears. Bigger clears and\n\
higher combos increase the spawn chance!"),
        ("🌍 Environmental Tiles", "\
• 🧊 Frozen — Contains a fruit but can't be matched directly.\n  \
  Clear a group next to it to thaw it into a normal fruit.\n\
• 🪨 Blocker — Indestructible stone. Only bombs and lightning\n  \
  can remove it. Cannot be swapped. Appears at higher levels."),
        ("💡 Tips", "\
• Plan big clears (5+ tiles) for exponential score gains\n\
• Save power-ups for when you need them most\n\
• Thaw frozen tiles early — they might contain the color you need\n\
• The combo multiplier is huge at 4+ — try to chain clears!"),
    ];

    for (heading, body) in sections {
        let h = Label::new(Some(heading));
        h.set_halign(gtk4::Align::Start);
        h.add_css_class("heading");
        h.set_margin_top(4);

        let b = Label::new(Some(body));
        b.set_halign(gtk4::Align::Start);
        b.set_wrap(true);
        b.set_max_width_chars(52);

        vbox.append(&h);
        vbox.append(&b);
    }

    let close_btn = Button::with_label("Got it!");
    close_btn.add_css_class("suggested-action");
    close_btn.set_halign(gtk4::Align::Center);
    close_btn.set_margin_top(8);
    {
        let d = dialog.clone();
        close_btn.connect_clicked(move |_| d.close());
    }
    vbox.append(&close_btn);

    let scroll = gtk4::ScrolledWindow::builder()
        .child(&vbox)
        .hexpand(true)
        .vexpand(true)
        .build();
    dialog.set_child(Some(&scroll));
    dialog.present();
}

// ── UI helpers ───────────────────────────────────────────────────────────────

fn update_labels(score_lbl: &Label, level_lbl: &Label, hiscore_lbl: &Label, game: &Game) {
    score_lbl.set_label(&format!(
        "Score: {} / {}",
        game.level_score, game.level_target
    ));
    level_lbl.set_label(&format!("Lv.{}", game.level));
    hiscore_lbl.set_label(&format!("Hi: {}", game.high_score));
}

fn combo_text(combo: u32) -> &'static str {
    match combo {
        0..=1 => "",
        2 => "  🔥 Double!",
        3 => "  🔥🔥 Triple!",
        4 => "  🔥🔥🔥 Quad!",
        _ => "  💥 MEGA COMBO!",
    }
}

// ── Grid / tile helpers ──────────────────────────────────────────────────────

fn make_tile_grid() -> (Grid, Vec<Vec<Button>>) {
    let grid = Grid::new();
    grid.set_row_spacing(4);
    grid.set_column_spacing(4);
    grid.set_halign(gtk4::Align::Center);
    grid.add_css_class("game-grid");

    let mut buttons = Vec::new();
    for row in 0..ROWS {
        let mut row_btns = Vec::new();
        for col in 0..COLS {
            let btn = Button::new();
            btn.add_css_class("tile");
            btn.set_size_request(50, 50);
            grid.attach(&btn, col as i32, row as i32, 1, 1);
            row_btns.push(btn);
        }
        buttons.push(row_btns);
    }
    (grid, buttons)
}

fn sync_grid(game: &Game, btns: &[Vec<Button>]) {
    for row in 0..ROWS {
        for col in 0..COLS {
            set_tile(&btns[row][col], game.grid[row][col]);
        }
    }
}

const ALL_TILE_CLASSES: &[&str] = &[
    "t0", "t1", "t2", "t3", "t4",
    "power-bomb", "power-lightning", "power-rainbow", "power-wildcard",
    "env-blocker", "frozen", "tile-glow", "tile-dim", "tile-pop", "tile-drop", "empty",
];

fn set_tile(btn: &Button, tile: Option<Tile>) {
    for cls in ALL_TILE_CLASSES {
        btn.remove_css_class(cls);
    }
    match tile {
        Some(t) => {
            btn.set_label(t.emoji());
            btn.add_css_class(t.css_class());
            if matches!(t, Tile::Frozen(_)) {
                btn.add_css_class("frozen");
            }
        }
        None => {
            btn.set_label("");
            btn.add_css_class("empty");
        }
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

// ── Animated removal + gravity ───────────────────────────────────────────────

fn animate_removal(
    group: &[(usize, usize)],
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    level_lbl: &Label,
    hiscore_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
    pts: u32,
    count: usize,
    combo: u32,
    leveled_up: bool,
) {
    animating.set(true);

    // Step 1: Pop animation on cleared tiles
    {
        let btns_ref = btns.borrow();
        for &(r, c) in group {
            btns_ref[r][c].add_css_class("tile-pop");
        }
    }

    let s = state.clone();
    let b = btns.clone();
    let sl = score_lbl.clone();
    let ll = level_lbl.clone();
    let hl = hiscore_lbl.clone();
    let stl = status_lbl.clone();
    let anim = animating.clone();

    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        sync_grid(&s.borrow(), &b.borrow());
        update_labels(&sl, &ll, &hl, &s.borrow());

        // Show power-up message if one was triggered, otherwise normal status
        let powerup_msg = s.borrow().last_powerup_msg.clone();
        s.borrow_mut().last_powerup_msg = None;
        let combo_str = combo_text(combo);
        let base_msg = format!("✨ {} tiles  +{} pts{}", count, pts, combo_str);
        let msg = match powerup_msg {
            Some(pm) => format!("{}  |  {}", base_msg, pm),
            None => base_msg,
        };
        stl.set_label(&msg);

        if leveled_up {
            let level = s.borrow().level;
            let empty_n = s.borrow().empty_count();
            stl.set_label(&format!(
                "🎉 LEVEL {}!  {} new tiles incoming!",
                level, empty_n
            ));
            s.borrow_mut().refill_empties();
            animate_fruit_rain(&s, &b, &sl, &ll, &hl, &stl, &anim);
        } else if s.borrow().game_over {
            let game = s.borrow();
            stl.set_label(&format!(
                "🏁 Game Over! Level {} — Total: {} — High: {}",
                game.level, game.score, game.high_score
            ));
            anim.set(false);
        } else {
            anim.set(false);
        }
    });
}

fn animate_fruit_rain(
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    level_lbl: &Label,
    hiscore_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
) {
    let game_snapshot = state.borrow().clone();
    let row_idx = Rc::new(Cell::new(ROWS as i32 - 1));

    let b = btns.clone();
    let s = state.clone();
    let sl = score_lbl.clone();
    let ll = level_lbl.clone();
    let hl = hiscore_lbl.clone();
    let stl = status_lbl.clone();
    let anim = animating.clone();

    glib::timeout_add_local(Duration::from_millis(60), move || {
        let r = row_idx.get();
        if r < 0 {
            sync_grid(&s.borrow(), &b.borrow());
            update_labels(&sl, &ll, &hl, &s.borrow());
            if s.borrow().has_move() {
                stl.set_label(&format!("🎉 Level {} — Keep going!", s.borrow().level));
            } else {
                s.borrow_mut().refill_empties();
                sync_grid(&s.borrow(), &b.borrow());
                stl.set_label("🍀 Extra refill — keep going!");
            }
            anim.set(false);
            return glib::ControlFlow::Break;
        }

        let row = r as usize;
        let btns_ref = b.borrow();
        for col in 0..COLS {
            if let Some(tile) = game_snapshot.grid[row][col] {
                let btn = &btns_ref[row][col];
                set_tile(btn, Some(tile));
                btn.add_css_class("tile-drop");
                let b2 = btn.clone();
                glib::timeout_add_local_once(Duration::from_millis(300), move || {
                    b2.remove_css_class("tile-drop");
                });
            }
        }

        row_idx.set(r - 1);
        glib::ControlFlow::Continue
    });
}

// ── Signal wiring ────────────────────────────────────────────────────────────

fn wire_tile(
    btn: &Button,
    row: usize,
    col: usize,
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    level_lbl: &Label,
    hiscore_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
) {
    // 1. Hover — highlight matching group (or power-up radius)
    let motion = EventControllerMotion::new();
    {
        let s_h = state.clone();
        let b_h = btns.clone();
        let anim_h = animating.clone();
        motion.connect_enter(move |_, _, _| {
            if anim_h.get() { return; }
            let game = s_h.borrow();
            if game.game_over { return; }
            let btns_ref = b_h.borrow();
            clear_highlights(&btns_ref);

            match game.grid[row][col] {
                // Power-up: highlight affected area
                Some(Tile::Bomb) => {
                    for dr in -1i32..=1 {
                        for dc in -1i32..=1 {
                            let r = row as i32 + dr;
                            let c = col as i32 + dc;
                            if r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
                                btns_ref[r as usize][c as usize].add_css_class("tile-glow");
                            }
                        }
                    }
                }
                Some(Tile::Lightning) => {
                    for c in 0..COLS {
                        btns_ref[row][c].add_css_class("tile-glow");
                    }
                }
                Some(Tile::Rainbow) => {
                    // Highlight the most common fruit color
                    let mut counts = [0u32; N_FRUITS];
                    for r in 0..ROWS {
                        for c in 0..COLS {
                            if let Some(Tile::Fruit(k)) = game.grid[r][c] {
                                counts[k as usize] += 1;
                            }
                        }
                    }
                    let target = counts.iter().enumerate()
                        .max_by_key(|(_, &n)| n)
                        .map(|(i, _)| i as u8)
                        .unwrap_or(0);
                    for r in 0..ROWS {
                        for c in 0..COLS {
                            match game.grid[r][c] {
                                Some(Tile::Fruit(k)) if k == target => {
                                    btns_ref[r][c].add_css_class("tile-glow");
                                }
                                Some(Tile::Rainbow) => {
                                    btns_ref[r][c].add_css_class("tile-glow");
                                }
                                _ if game.grid[r][c].is_some() => {
                                    btns_ref[r][c].add_css_class("tile-dim");
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {
                    // Normal flood highlight
                    let group = game.flood(row, col);
                    if group.len() >= MIN_GROUP {
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
                }
            }
        });
    }

    {
        let b_l = btns.clone();
        let anim_l = animating.clone();
        motion.connect_leave(move |_| {
            if !anim_l.get() { clear_highlights(&b_l.borrow()); }
        });
    }
    btn.add_controller(motion);

    // 2. Left Click — clear group or activate power-up
    {
        let s_c = state.clone();
        let b_c = btns.clone();
        let sl_c = score_lbl.clone();
        let ll_c = level_lbl.clone();
        let hl_c = hiscore_lbl.clone();
        let stl_c = status_lbl.clone();
        let anim_c = animating.clone();
        btn.connect_clicked(move |_| {
            if anim_c.get() { return; }

            // For power-ups, compute affected area before activation
            let is_powerup = matches!(
                s_c.borrow().grid[row][col],
                Some(Tile::Bomb | Tile::Lightning | Tile::Rainbow)
            );

            let affected: Vec<(usize, usize)> = if is_powerup {
                // Compute what will be cleared
                let game = s_c.borrow();
                match game.grid[row][col] {
                    Some(Tile::Bomb) => {
                        let mut v = Vec::new();
                        for dr in -1i32..=1 {
                            for dc in -1i32..=1 {
                                let r = row as i32 + dr;
                                let c = col as i32 + dc;
                                if r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
                                    if game.grid[r as usize][c as usize].is_some() {
                                        v.push((r as usize, c as usize));
                                    }
                                }
                            }
                        }
                        v
                    }
                    Some(Tile::Lightning) => {
                        (0..COLS).filter(|&c| game.grid[row][c].is_some())
                            .map(|c| (row, c)).collect()
                    }
                    Some(Tile::Rainbow) => {
                        let mut counts = [0u32; N_FRUITS];
                        for r in 0..ROWS {
                            for c in 0..COLS {
                                if let Some(Tile::Fruit(k)) = game.grid[r][c] {
                                    counts[k as usize] += 1;
                                }
                            }
                        }
                        let target = counts.iter().enumerate()
                            .max_by_key(|(_, &n)| n)
                            .map(|(i, _)| i as u8).unwrap_or(0);
                        let mut v = Vec::new();
                        for r in 0..ROWS {
                            for c in 0..COLS {
                                match game.grid[r][c] {
                                    Some(Tile::Fruit(k)) if k == target => v.push((r, c)),
                                    Some(Tile::Rainbow) => v.push((r, c)),
                                    _ => {}
                                }
                            }
                        }
                        v
                    }
                    _ => vec![],
                }
            } else {
                s_c.borrow().flood(row, col)
            };

            let result = s_c.borrow_mut().click(row, col);
            match result {
                None => {
                    if !s_c.borrow().game_over {
                        stl_c.set_label("⚠️  Need 3+ connected matching tiles!");
                    }
                }
                Some((n, pts, leveled_up)) => {
                    let combo = s_c.borrow().combo;
                    clear_highlights(&b_c.borrow());
                    animate_removal(
                        &affected, &s_c, &b_c, &sl_c, &ll_c, &hl_c, &stl_c,
                        &anim_c, pts, n, combo, leveled_up,
                    );
                }
            }
        });
    }

    // 3. Right Click — swap mechanic (validated)
    let gesture = GestureClick::new();
    gesture.set_button(3);
    {
        let s_s = state.clone();
        let b_s = btns.clone();
        let sl_s = score_lbl.clone();
        let ll_s = level_lbl.clone();
        let hl_s = hiscore_lbl.clone();
        let stl_s = status_lbl.clone();
        let anim_s = animating.clone();

        gesture.connect_pressed(move |_, _, _, _| {
            if anim_s.get() { return; }
            let mut game = s_s.borrow_mut();
            if game.game_over { return; }

            if let Some(selected) = game.selected_pos {
                match game.swap_tiles(selected.0, selected.1, row, col) {
                    Ok(()) => {
                        game.selected_pos = None;
                        update_labels(&sl_s, &ll_s, &hl_s, &game);
                        stl_s.set_label("🔄 Swapped!");
                        sync_grid(&game, &b_s.borrow());
                        if game.game_over {
                            stl_s.set_label(&format!(
                                "🏁 Game Over! Level {} — Total: {} — High: {}",
                                game.level, game.score, game.high_score
                            ));
                        }
                    }
                    Err(reason) => {
                        stl_s.set_label(&format!("⚠️ {}", reason));
                        game.selected_pos = None;
                        clear_highlights(&b_s.borrow());
                    }
                }
            } else if game.grid[row][col].is_some() {
                game.selected_pos = Some((row, col));
                clear_highlights(&b_s.borrow());
                b_s.borrow()[row][col].add_css_class("tile-glow");
                stl_s.set_label("🔀 Right-click an adjacent tile to swap");
            }
        });
    }
    btn.add_controller(gesture);
}

// ── CSS ──────────────────────────────────────────────────────────────────────

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

.game-grid {
    background-color: #0d0d1a;
    padding: 12px;
    border-radius: 12px;
}

.tile {
    border-radius: 8px;
    min-width: 50px;
    min-height: 50px;
    padding: 0;
    transition: background-color 0.2s ease-out,
                opacity 0.2s ease-out;
}

.tile label { font-size: 24px; }

/* ── Fruit colors ── */
.tile.t0 { background-color: #c0392b; color: white; }
.tile.t1 { background-color: #d35400; color: white; }
.tile.t2 { background-color: #c9a800; color: #222;  }
.tile.t3 { background-color: #7d3c98; color: white; }
.tile.t4 { background-color: #1a5276; color: white; }

/* ── Power-ups ── */
.tile.power-bomb {
    background: radial-gradient(circle, #ff6b35 30%, #c0392b 100%);
    color: white;
}
.tile.power-lightning {
    background: linear-gradient(135deg, #f9d423 0%, #ff4e00 100%);
    color: #222;
}
.tile.power-rainbow {
    background: linear-gradient(135deg, #ff0000, #ff8800, #ffff00, #00ff00, #0088ff, #8800ff);
    color: white;
}
.tile.power-wildcard {
    background: linear-gradient(135deg, #2c3e50 0%, #3498db 50%, #2c3e50 100%);
    color: white;
}

/* ── Environmental ── */
.tile.frozen {
    opacity: 0.75;
    outline: 2px solid #89CFF0;
    outline-offset: -2px;
}
.tile.env-blocker {
    background-color: #2c2c3e;
    color: #888;
}

/* ── States ── */
.tile:hover {
    background-color: alpha(white, 0.12);
}
.tile.empty {
    background-color: #0a0a16;
    opacity: 0.08;
}
.tile.tile-glow {
    outline: 2px solid rgba(255, 255, 255, 0.85);
    outline-offset: -2px;
    background-image: linear-gradient(rgba(255,255,255,0.18), rgba(255,255,255,0.0));
}
.tile.tile-dim { opacity: 0.2; }

.tile.tile-pop {
    opacity: 0;
    background-color: white;
}

.tile.tile-drop {
    animation: tile-land 0.3s ease-out;
}

@keyframes tile-land {
    0%   { opacity: 0; }
    50%  { opacity: 0.7; }
    100% { opacity: 1; }
}
"#;

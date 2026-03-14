use gtk4::prelude::*;
use gtk4::glib;
use gtk4::{
    Box as GtkBox, Button, CssProvider, Fixed, GestureClick, Label, Orientation,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::cell::RefCell;
use std::rc::Rc;

// ── Layout constants ──────────────────────────────────────────────────────────
const CARD_W: i32 = 73;
const CARD_H: i32 = 100;
const PILE_STEP: i32 = 106;
const MARGIN_X: i32 = 20;
const MARGIN_Y: i32 = 16;
const TOP_ROW_Y: i32 = MARGIN_Y;
const TABLEAU_Y: i32 = MARGIN_Y + CARD_H + 30; // 146
const FACE_DOWN_GAP: i32 = 17;
const FACE_UP_GAP: i32 = 26;

fn pile_x(col: usize) -> i32 {
    MARGIN_X + col as i32 * PILE_STEP
}

// ── Card types ────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Debug)]
enum Suit {
    Spades = 0,
    Hearts = 1,
    Diamonds = 2,
    Clubs = 3,
}

impl Suit {
    fn symbol(self) -> &'static str {
        match self {
            Suit::Spades => "♠",
            Suit::Hearts => "♥",
            Suit::Diamonds => "♦",
            Suit::Clubs => "♣",
        }
    }
    fn is_red(self) -> bool {
        matches!(self, Suit::Hearts | Suit::Diamonds)
    }
}

fn rank_str(rank: u8) -> &'static str {
    match rank {
        1 => "A",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "10",
        11 => "J",
        12 => "Q",
        13 => "K",
        _ => "?",
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Card {
    suit: Suit,
    rank: u8,
    face_up: bool,
}

// ── Game state ────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct GameState {
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; 4],
    tableau: [Vec<Card>; 7],
    score: i32,
    moves: u32,
}

impl GameState {
    fn new() -> Self {
        let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
        let mut deck: Vec<Card> = suits
            .iter()
            .flat_map(|&suit| {
                (1u8..=13).map(move |rank| Card {
                    suit,
                    rank,
                    face_up: false,
                })
            })
            .collect();

        // Fisher-Yates shuffle via glib
        for i in (1..deck.len()).rev() {
            let j = glib::random_int_range(0, (i + 1) as i32) as usize;
            deck.swap(i, j);
        }

        let mut tableau: [Vec<Card>; 7] = std::array::from_fn(|_| Vec::new());
        let mut idx = 0usize;
        for col in 0..7 {
            for _ in 0..=col {
                tableau[col].push(deck[idx]);
                idx += 1;
            }
            if let Some(top) = tableau[col].last_mut() {
                top.face_up = true;
            }
        }

        let stock: Vec<Card> = deck[idx..].to_vec();

        GameState {
            stock,
            waste: Vec::new(),
            foundations: std::array::from_fn(|_| Vec::new()),
            tableau,
            score: 0,
            moves: 0,
        }
    }

    fn can_place_on_foundation(&self, card: Card, fi: usize) -> bool {
        let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
        if card.suit != suits[fi] {
            return false;
        }
        match self.foundations[fi].last() {
            None => card.rank == 1,
            Some(top) => card.rank == top.rank + 1,
        }
    }

    fn can_place_on_tableau(&self, card: Card, col: usize) -> bool {
        match self.tableau[col].last() {
            None => card.rank == 13,
            Some(top) => {
                top.face_up
                    && card.rank + 1 == top.rank
                    && card.suit.is_red() != top.suit.is_red()
            }
        }
    }

    fn is_won(&self) -> bool {
        self.foundations.iter().all(|f| f.len() == 13)
    }

    fn auto_move_to_foundation(&mut self, col: usize) -> bool {
        let card = match self.tableau[col].last().copied() {
            Some(c) if c.face_up => c,
            _ => return false,
        };
        for fi in 0..4 {
            if self.can_place_on_foundation(card, fi) {
                self.tableau[col].pop();
                if let Some(top) = self.tableau[col].last_mut() {
                    top.face_up = true;
                }
                self.foundations[fi].push(card);
                self.score += 10;
                self.moves += 1;
                return true;
            }
        }
        false
    }
}

// ── Hit testing ───────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Debug)]
enum PileId {
    Stock,
    Waste,
    Foundation(usize),
    Tableau(usize),
}

#[derive(Clone, Copy, Debug)]
struct HitResult {
    pile: PileId,
    card_idx: usize,
}

fn card_y_in_tableau(pile: &[Card], idx: usize) -> i32 {
    let mut y = 0i32;
    for i in 0..idx {
        if pile[i].face_up {
            y += FACE_UP_GAP;
        } else {
            y += FACE_DOWN_GAP;
        }
    }
    y
}

fn hit_test(x: f64, y: f64, state: &GameState) -> Option<HitResult> {
    let xi = x as i32;
    let yi = y as i32;

    let in_card = |cx: i32, cy: i32| -> bool {
        xi >= cx && xi < cx + CARD_W && yi >= cy && yi < cy + CARD_H
    };

    // Stock
    if in_card(pile_x(0), TOP_ROW_Y) {
        return Some(HitResult {
            pile: PileId::Stock,
            card_idx: state.stock.len().saturating_sub(1),
        });
    }

    // Waste
    if !state.waste.is_empty() && in_card(pile_x(1), TOP_ROW_Y) {
        return Some(HitResult {
            pile: PileId::Waste,
            card_idx: state.waste.len() - 1,
        });
    }

    // Foundations (cols 3-6)
    for fi in 0..4 {
        let cx = pile_x(3 + fi);
        if in_card(cx, TOP_ROW_Y) {
            return Some(HitResult {
                pile: PileId::Foundation(fi),
                card_idx: state.foundations[fi].len().saturating_sub(1),
            });
        }
    }

    // Tableau — last hit wins (topmost card)
    for col in 0..7 {
        let pile = &state.tableau[col];
        if pile.is_empty() {
            if in_card(pile_x(col), TABLEAU_Y) {
                return Some(HitResult {
                    pile: PileId::Tableau(col),
                    card_idx: 0,
                });
            }
            continue;
        }
        let mut hit: Option<HitResult> = None;
        for i in 0..pile.len() {
            let cy = TABLEAU_Y + card_y_in_tableau(pile, i);
            let card_height = if i + 1 == pile.len() {
                CARD_H
            } else if pile[i].face_up {
                FACE_UP_GAP
            } else {
                FACE_DOWN_GAP
            };
            if xi >= pile_x(col)
                && xi < pile_x(col) + CARD_W
                && yi >= cy
                && yi < cy + card_height
            {
                hit = Some(HitResult {
                    pile: PileId::Tableau(col),
                    card_idx: i,
                });
            }
        }
        if hit.is_some() {
            return hit;
        }
    }

    None
}

// ── App state ─────────────────────────────────────────────────────────────────
struct AppState {
    game: GameState,
    selection: Option<HitResult>,
    undo_stack: Vec<GameState>,
    board: Fixed,
    score_lbl: Label,
    moves_lbl: Label,
}

impl AppState {
    fn new(board: Fixed, score_lbl: Label, moves_lbl: Label) -> Self {
        AppState {
            game: GameState::new(),
            selection: None,
            undo_stack: Vec::new(),
            board,
            score_lbl,
            moves_lbl,
        }
    }

    fn save_undo(&mut self) {
        self.undo_stack.push(self.game.clone());
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    fn update_labels(&self) {
        self.score_lbl
            .set_text(&format!("Score: {}", self.game.score));
        self.moves_lbl
            .set_text(&format!("Moves: {}", self.game.moves));
    }
}

// ── CSS ───────────────────────────────────────────────────────────────────────
const CSS: &str = r#"
.game-board {
    background-color: #2d6a4f;
    background-image: radial-gradient(ellipse at 30% 30%, #3a8a63 0%, #2d6a4f 60%, #1e4d38 100%);
}

.card-face {
    background-color: #fffef8;
    border-radius: 7px;
    border: 1px solid #ccc8b8;
    box-shadow: 2px 3px 6px rgba(0,0,0,0.35), inset 0 1px 0 rgba(255,255,255,0.8);
}

.card-back {
    background-color: #1a237e;
    background-image:
        repeating-linear-gradient(
            45deg,
            rgba(255,255,255,0.04) 0px,
            rgba(255,255,255,0.04) 2px,
            transparent 2px,
            transparent 8px
        ),
        repeating-linear-gradient(
            -45deg,
            rgba(255,255,255,0.04) 0px,
            rgba(255,255,255,0.04) 2px,
            transparent 2px,
            transparent 8px
        );
    border-radius: 7px;
    border: 1px solid #0d1557;
    box-shadow: 2px 3px 6px rgba(0,0,0,0.4);
}

.card-empty {
    border-radius: 7px;
    border: 2px dashed rgba(255,255,255,0.25);
    background-color: rgba(0,0,0,0.12);
}

.card-selected {
    border: 2px solid #ffd700;
    box-shadow: 0 0 0 2px #ffd700, 2px 3px 8px rgba(0,0,0,0.5);
}

.corner-label {
    font-size: 11px;
    font-weight: bold;
    padding: 2px 3px;
}

.center-suit {
    font-size: 32px;
}

.card-red {
    color: #cc0000;
}

.card-black {
    color: #111111;
}

.hint-label {
    color: rgba(255,255,255,0.3);
    font-size: 22px;
    font-weight: bold;
}

.win-banner {
    background-color: rgba(0,0,0,0.72);
    border-radius: 18px;
    border: 2px solid #ffd700;
    padding: 24px 48px;
}

.win-text {
    color: #ffd700;
    font-size: 36px;
    font-weight: bold;
}

.win-sub {
    color: #ffffff;
    font-size: 18px;
}

.score-label {
    color: #e8e8e8;
    font-size: 13px;
    font-weight: bold;
    margin-left: 6px;
    margin-right: 6px;
}

.header-button {
    margin-left: 4px;
    margin-right: 4px;
}
"#;

// ── Widget builders ───────────────────────────────────────────────────────────
fn make_card_widget(card: Card, selected: bool) -> GtkBox {
    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_size_request(CARD_W, CARD_H);

    if card.face_up {
        outer.add_css_class("card-face");
        if selected {
            outer.add_css_class("card-selected");
        }

        let color_class = if card.suit.is_red() { "card-red" } else { "card-black" };
        let corner_text = format!("{}{}", rank_str(card.rank), card.suit.symbol());

        // Top-left corner
        let top_row = GtkBox::new(Orientation::Horizontal, 0);
        let corner = Label::new(Some(&corner_text));
        corner.add_css_class("corner-label");
        corner.add_css_class(color_class);
        corner.set_halign(gtk4::Align::Start);
        top_row.append(&corner);
        outer.append(&top_row);

        // Center suit
        let mid = GtkBox::new(Orientation::Horizontal, 0);
        mid.set_vexpand(true);
        mid.set_valign(gtk4::Align::Center);
        mid.set_halign(gtk4::Align::Center);
        let suit_lbl = Label::new(Some(card.suit.symbol()));
        suit_lbl.add_css_class("center-suit");
        suit_lbl.add_css_class(color_class);
        mid.append(&suit_lbl);
        outer.append(&mid);

        // Bottom-right corner
        let bot_row = GtkBox::new(Orientation::Horizontal, 0);
        let corner2 = Label::new(Some(&corner_text));
        corner2.add_css_class("corner-label");
        corner2.add_css_class(color_class);
        corner2.set_halign(gtk4::Align::End);
        corner2.set_hexpand(true);
        bot_row.append(&corner2);
        outer.append(&bot_row);
    } else {
        outer.add_css_class("card-back");
    }

    outer
}

fn make_empty_slot(hint: &str) -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 0);
    b.set_size_request(CARD_W, CARD_H);
    b.add_css_class("card-empty");
    if !hint.is_empty() {
        let lbl = Label::new(Some(hint));
        lbl.add_css_class("hint-label");
        lbl.set_halign(gtk4::Align::Center);
        lbl.set_valign(gtk4::Align::Center);
        lbl.set_vexpand(true);
        b.append(&lbl);
    }
    b
}

// ── Redraw ────────────────────────────────────────────────────────────────────
fn redraw(state: &AppState) {
    let board = &state.board;

    // Clear all children
    while let Some(child) = board.first_child() {
        board.remove(&child);
    }

    // Stock
    let stock_w: GtkBox = if state.game.stock.is_empty() {
        make_empty_slot("↺")
    } else {
        // Show back of top stock card
        make_card_widget(
            Card { suit: Suit::Spades, rank: 1, face_up: false },
            false,
        )
    };
    board.put(&stock_w, pile_x(0) as f64, TOP_ROW_Y as f64);

    // Waste
    let waste_w: GtkBox = if let Some(&top) = state.game.waste.last() {
        let sel = matches!(state.selection, Some(s) if s.pile == PileId::Waste);
        make_card_widget(top, sel)
    } else {
        make_empty_slot("")
    };
    board.put(&waste_w, pile_x(1) as f64, TOP_ROW_Y as f64);

    // Foundations (suit order: Spades, Hearts, Diamonds, Clubs → cols 3-6)
    let suit_hints = ["♠", "♥", "♦", "♣"];
    for fi in 0..4 {
        let fw: GtkBox = if let Some(&top) = state.game.foundations[fi].last() {
            let sel = matches!(state.selection, Some(s) if s.pile == PileId::Foundation(fi));
            make_card_widget(top, sel)
        } else {
            make_empty_slot(suit_hints[fi])
        };
        board.put(&fw, pile_x(3 + fi) as f64, TOP_ROW_Y as f64);
    }

    // Tableau
    for col in 0..7 {
        let pile = &state.game.tableau[col];
        if pile.is_empty() {
            let ew = make_empty_slot("K");
            board.put(&ew, pile_x(col) as f64, TABLEAU_Y as f64);
            continue;
        }
        let sel_idx = match state.selection {
            Some(s) if s.pile == PileId::Tableau(col) => Some(s.card_idx),
            _ => None,
        };
        for i in 0..pile.len() {
            let cy = TABLEAU_Y + card_y_in_tableau(pile, i);
            let card = pile[i];
            let selected = card.face_up && sel_idx.map_or(false, |si| i >= si);
            let cw = make_card_widget(card, selected);
            board.put(&cw, pile_x(col) as f64, cy as f64);
        }
    }

    // Win overlay
    if state.game.is_won() {
        let overlay = GtkBox::new(Orientation::Vertical, 12);
        overlay.set_size_request(360, 160);
        overlay.add_css_class("win-banner");
        overlay.set_halign(gtk4::Align::Center);
        overlay.set_valign(gtk4::Align::Center);

        let win_lbl = Label::new(Some("You Win! 🎉"));
        win_lbl.add_css_class("win-text");
        overlay.append(&win_lbl);

        let score_lbl = Label::new(Some(&format!("Final Score: {}", state.game.score)));
        score_lbl.add_css_class("win-sub");
        overlay.append(&score_lbl);

        board.put(&overlay, 200.0, 220.0);
    }

    state.update_labels();
}

// ── Move execution ────────────────────────────────────────────────────────────
fn do_place(state: &mut AppState, target: PileId) -> bool {
    let sel = match state.selection {
        Some(s) => s,
        None => return false,
    };

    let cards_to_move: Vec<Card> = match sel.pile {
        PileId::Waste => match state.game.waste.last() {
            Some(&c) => vec![c],
            None => return false,
        },
        PileId::Foundation(fi) => match state.game.foundations[fi].last() {
            Some(&c) => vec![c],
            None => return false,
        },
        PileId::Tableau(col) => {
            if sel.card_idx >= state.game.tableau[col].len() {
                return false;
            }
            state.game.tableau[col][sel.card_idx..].to_vec()
        }
        PileId::Stock => return false,
    };

    if cards_to_move.is_empty() {
        return false;
    }

    let valid = match target {
        PileId::Foundation(fi) => {
            cards_to_move.len() == 1
                && state.game.can_place_on_foundation(cards_to_move[0], fi)
        }
        PileId::Tableau(col) => state.game.can_place_on_tableau(cards_to_move[0], col),
        _ => return false,
    };

    if !valid {
        return false;
    }

    state.save_undo();

    // Remove from source
    match sel.pile {
        PileId::Waste => {
            state.game.waste.pop();
        }
        PileId::Foundation(fi) => {
            state.game.foundations[fi].pop();
        }
        PileId::Tableau(col) => {
            state.game.tableau[col].truncate(sel.card_idx);
            if let Some(top) = state.game.tableau[col].last_mut() {
                if !top.face_up {
                    top.face_up = true;
                    state.game.score += 5;
                }
            }
        }
        PileId::Stock => {}
    }

    // Add to target
    match target {
        PileId::Foundation(fi) => {
            for c in &cards_to_move {
                state.game.foundations[fi].push(*c);
            }
            state.game.score += 10 * cards_to_move.len() as i32;
        }
        PileId::Tableau(col) => {
            for c in &cards_to_move {
                state.game.tableau[col].push(*c);
            }
            if sel.pile == PileId::Waste {
                state.game.score += 5;
            }
        }
        _ => {}
    }

    state.game.moves += 1;
    state.selection = None;
    true
}

fn handle_click(state: &mut AppState, x: f64, y: f64, double_click: bool) {
    if double_click {
        if let Some(hit) = hit_test(x, y, &state.game) {
            match hit.pile {
                PileId::Tableau(col) => {
                    if state.game.auto_move_to_foundation(col) {
                        state.selection = None;
                    }
                }
                PileId::Waste => {
                    if let Some(top) = state.game.waste.last().copied() {
                        for fi in 0..4 {
                            if state.game.can_place_on_foundation(top, fi) {
                                state.save_undo();
                                state.game.waste.pop();
                                state.game.foundations[fi].push(top);
                                state.game.score += 10;
                                state.game.moves += 1;
                                state.selection = None;
                                return;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        return;
    }

    let hit = hit_test(x, y, &state.game);

    // Stock click
    if let Some(h) = hit {
        if h.pile == PileId::Stock {
            if state.game.stock.is_empty() {
                state.save_undo();
                let mut new_stock: Vec<Card> = state.game.waste.drain(..).collect();
                new_stock.reverse();
                for c in &mut new_stock {
                    c.face_up = false;
                }
                state.game.stock = new_stock;
                state.game.score = (state.game.score - 100).max(0);
            } else {
                state.save_undo();
                if let Some(mut c) = state.game.stock.pop() {
                    c.face_up = true;
                    state.game.waste.push(c);
                    state.game.score += 5;
                    state.game.moves += 1;
                }
            }
            state.selection = None;
            return;
        }
    }

    // Have a selection
    if let Some(sel) = state.selection {
        match hit {
            None => {
                state.selection = None;
            }
            Some(h) => {
                // Same card: deselect
                if h.pile == sel.pile && h.card_idx == sel.card_idx {
                    state.selection = None;
                    return;
                }

                match h.pile {
                    PileId::Foundation(fi) => {
                        if !do_place(state, PileId::Foundation(fi)) {
                            // Try selecting foundation top instead
                            if !state.game.foundations[fi].is_empty() {
                                state.selection = Some(HitResult {
                                    pile: PileId::Foundation(fi),
                                    card_idx: state.game.foundations[fi].len() - 1,
                                });
                            } else {
                                state.selection = None;
                            }
                        }
                    }
                    PileId::Tableau(col) => {
                        if !do_place(state, PileId::Tableau(col)) {
                            let pile = &state.game.tableau[col];
                            if h.card_idx < pile.len() && pile[h.card_idx].face_up {
                                state.selection = Some(h);
                            } else {
                                state.selection = None;
                            }
                        }
                    }
                    PileId::Waste => {
                        if !state.game.waste.is_empty() {
                            state.selection = Some(HitResult {
                                pile: PileId::Waste,
                                card_idx: state.game.waste.len() - 1,
                            });
                        } else {
                            state.selection = None;
                        }
                    }
                    PileId::Stock => {
                        state.selection = None;
                    }
                }
            }
        }
        return;
    }

    // No selection — try to select
    match hit {
        None => {
            state.selection = None;
        }
        Some(h) => match h.pile {
            PileId::Waste => {
                if !state.game.waste.is_empty() {
                    state.selection = Some(HitResult {
                        pile: PileId::Waste,
                        card_idx: state.game.waste.len() - 1,
                    });
                }
            }
            PileId::Tableau(col) => {
                let pile = &state.game.tableau[col];
                if h.card_idx < pile.len() && pile[h.card_idx].face_up {
                    state.selection = Some(h);
                }
            }
            PileId::Foundation(fi) => {
                if !state.game.foundations[fi].is_empty() {
                    state.selection = Some(HitResult {
                        pile: PileId::Foundation(fi),
                        card_idx: state.game.foundations[fi].len() - 1,
                    });
                }
            }
            PileId::Stock => {}
        },
    }
}

// ── Public entry point ────────────────────────────────────────────────────────
pub fn connect_handlers(builder: &gtk4::Builder, _window: &gtk4::ApplicationWindow) {
    // Load CSS
    let provider = CssProvider::new();
    provider.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display"),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Get the GtkFixed game board from builder
    let board: Fixed = builder
        .object("game_board")
        .expect("game_board widget not found in UI");

    // Build header bar widgets
    let score_lbl = Label::new(Some("Score: 0"));
    score_lbl.add_css_class("score-label");

    let moves_lbl = Label::new(Some("Moves: 0"));
    moves_lbl.add_css_class("score-label");

    let new_game_btn = Button::with_label("New Game");
    new_game_btn.add_css_class("header-button");

    let undo_btn = Button::with_label("Undo");
    undo_btn.add_css_class("header-button");

    if let Some(hb) = builder.object::<gtk4::HeaderBar>("header_bar") {
        hb.pack_start(&score_lbl);
        hb.pack_start(&moves_lbl);
        hb.pack_end(&new_game_btn);
        hb.pack_end(&undo_btn);
    }

    // Shared state
    let state = Rc::new(RefCell::new(AppState::new(
        board.clone(),
        score_lbl,
        moves_lbl,
    )));

    // Initial draw
    redraw(&state.borrow());

    // Click handler — single and double click
    let click = GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);

    {
        let state = state.clone();
        click.connect_pressed(move |gesture, n_press, x, y| {
            if n_press >= 2 {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                handle_click(&mut state.borrow_mut(), x, y, true);
                redraw(&state.borrow());
            }
        });
    }

    {
        let state = state.clone();
        click.connect_released(move |gesture, n_press, x, y| {
            if n_press == 1 {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                handle_click(&mut state.borrow_mut(), x, y, false);
                redraw(&state.borrow());
            }
        });
    }

    board.add_controller(click);

    // New Game button
    {
        let state = state.clone();
        new_game_btn.connect_clicked(move |_| {
            {
                let mut s = state.borrow_mut();
                s.game = GameState::new();
                s.selection = None;
                s.undo_stack.clear();
            }
            redraw(&state.borrow());
        });
    }

    // Undo button
    {
        let state = state.clone();
        undo_btn.connect_clicked(move |_| {
            {
                let mut s = state.borrow_mut();
                if let Some(prev) = s.undo_stack.pop() {
                    s.game = prev;
                    s.selection = None;
                }
            }
            redraw(&state.borrow());
        });
    }
}

//! Escape sequence conformance of the grid engine: fixed-output tests that feed a byte script
//! to [`TermGrid`] and compare the screen, the cursor, the attributes and the replies.
//!
//! The cases are written from the specifications (ECMA-48 and ECMA-35, the xterm control
//! sequences document, the DEC VT100 and VT220 manuals), in the groups that vttest and esctest
//! test: cursor movement, scrolling regions, insert and delete, tabs, character sets, SGR,
//! modes, reports, the title stack, resizing and reflow. No code or text of either suite was
//! used. What the engine (`alacritty_terminal`) does differently from the specification is
//! not asserted as correct: it is listed in `deviations()` as the behaviour the engine has, so
//! that a new version of the engine that changes it shows up here. See docs/conformance.md of
//! the Terminal app for the results.

use super::*;

fn term(cols: u16, rows: u16) -> TermGrid {
    TermGrid::new(cols, rows)
}

/// A terminal that has been fed `input`.
fn run(cols: u16, rows: u16, input: &str) -> TermGrid {
    let mut grid = term(cols, rows);
    grid.advance(input.as_bytes());
    grid
}

fn screen(grid: &TermGrid) -> Vec<String> {
    grid.screen()
}

fn cursor(grid: &TermGrid) -> (usize, usize) {
    grid.cursor_cell()
}

fn replies(grid: &mut TermGrid) -> String {
    String::from_utf8_lossy(&grid.take_replies()).into_owned()
}

/// The runs `[text, fg, bg, flags]` of row `y` after a full frame.
fn runs_of(grid: &mut TermGrid, y: usize) -> Vec<Value> {
    grid.full = true;
    let frame = grid.frame(1).unwrap_or(Value::Null);
    frame["lines"]
        .as_array()
        .and_then(|lines| lines.iter().find(|l| l["y"] == y))
        .and_then(|l| l["runs"].as_array())
        .cloned()
        .unwrap_or_default()
}

fn rows(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| (*l).to_owned()).collect()
}

// ---- cursor movement (vttest 1) ------------------------------------------------------

#[test]
fn cup_and_hvp_position_the_cursor_one_based_with_defaults() {
    assert_eq!(cursor(&run(20, 10, "\x1b[3;5H")), (4, 2));
    assert_eq!(cursor(&run(20, 10, "\x1b[3;5f")), (4, 2));
    assert_eq!(cursor(&run(20, 10, "\x1b[5;5H\x1b[H")), (0, 0));
    assert_eq!(cursor(&run(20, 10, "\x1b[;5H")), (4, 0));
    assert_eq!(cursor(&run(20, 10, "\x1b[4;H")), (0, 3));
    // Zero means one.
    assert_eq!(cursor(&run(20, 10, "\x1b[0;0H")), (0, 0));
    // Out of range is clamped to the screen.
    assert_eq!(cursor(&run(20, 10, "\x1b[99;99H")), (19, 9));
}

#[test]
fn relative_moves_default_to_one_clamp_and_stop_at_the_margins() {
    let at = |seq: &str| cursor(&run(20, 10, &format!("\x1b[5;10H{seq}")));
    assert_eq!(at("\x1b[A"), (9, 3));
    assert_eq!(at("\x1b[0A"), (9, 3));
    assert_eq!(at("\x1b[2A"), (9, 2));
    assert_eq!(at("\x1b[B"), (9, 5));
    assert_eq!(at("\x1b[3B"), (9, 7));
    assert_eq!(at("\x1b[C"), (10, 4));
    assert_eq!(at("\x1b[4C"), (13, 4));
    assert_eq!(at("\x1b[D"), (8, 4));
    assert_eq!(at("\x1b[4D"), (5, 4));
    assert_eq!(at("\x1b[99A"), (9, 0));
    assert_eq!(at("\x1b[99B"), (9, 9));
    assert_eq!(at("\x1b[99C"), (19, 4));
    assert_eq!(at("\x1b[99D"), (0, 4));
}

#[test]
fn absolute_column_and_row_and_next_previous_line() {
    let at = |seq: &str| cursor(&run(20, 10, &format!("\x1b[5;10H{seq}")));
    assert_eq!(at("\x1b[3G"), (2, 4));
    assert_eq!(at("\x1b[G"), (0, 4));
    assert_eq!(at("\x1b[3`"), (2, 4)); // HPA
    assert_eq!(at("\x1b[2a"), (11, 4)); // HPR
    assert_eq!(at("\x1b[2d"), (9, 1)); // VPA
    assert_eq!(at("\x1b[2e"), (9, 6)); // VPR
    assert_eq!(at("\x1b[2E"), (0, 6)); // CNL
    assert_eq!(at("\x1b[2F"), (0, 2)); // CPL
}

#[test]
fn carriage_return_line_feed_backspace_and_friends() {
    let grid = run(10, 4, "ab\r\ncd\x08\x08x");
    assert_eq!(screen(&grid), rows(&["ab", "xd", "", ""]));
    // LF alone does not return the carriage; VT and FF are line feeds too.
    let grid = run(10, 4, "ab\ncd\x0bef\x0cgh");
    assert_eq!(screen(&grid), rows(&["ab", "  cd", "    ef", "      gh"]));
    // BS at the left edge stays.
    assert_eq!(cursor(&run(10, 4, "\x08\x08")), (0, 0));
    // NUL and DEL print nothing and do not move.
    assert_eq!(cursor(&run(10, 4, "a\x00\x7fb")), (2, 0));
    assert_eq!(screen(&run(10, 4, "a\x00\x7fb")), rows(&["ab", "", "", ""]));
}

#[test]
fn index_next_line_and_reverse_index_scroll_at_the_margins() {
    // IND at the bottom scrolls up.
    let grid = run(5, 3, "a\r\nb\r\nc\x1bD");
    assert_eq!(screen(&grid), rows(&["b", "c", ""]));
    assert_eq!(cursor(&grid), (1, 2));
    // NEL is IND plus CR.
    let grid = run(5, 3, "a\r\nb\r\ncde\x1bE");
    assert_eq!(screen(&grid), rows(&["b", "cde", ""]));
    assert_eq!(cursor(&grid), (0, 2));
    // RI at the top scrolls down.
    let grid = run(5, 3, "a\r\nb\r\nc\x1b[H\x1bM");
    assert_eq!(screen(&grid), rows(&["", "a", "b"]));
    // RI elsewhere only moves up.
    assert_eq!(cursor(&run(5, 3, "\x1b[3;1H\x1bM")), (0, 1));
}

#[test]
fn save_and_restore_cursor_keep_position_and_rendition() {
    let mut grid = run(20, 5, "\x1b[2;4H\x1b[31;1m\x1b7\x1b[4;9H\x1b[0mz\x1b8Q");
    assert_eq!(screen(&grid)[1], "   Q");
    assert_eq!(screen(&grid)[3], "        z");
    // The restored rendition is red and bold again.
    let runs = runs_of(&mut grid, 1);
    let q = runs.last().cloned().unwrap_or(Value::Null);
    assert_eq!(q[0], "Q");
    assert_eq!(q[1], json!(PALETTE + 1));
    assert_eq!(q[3], 1);
    // Without a save, restore goes home.
    assert_eq!(cursor(&run(20, 5, "\x1b[3;3H\x1b8")), (0, 0));
    // CSI s and CSI u do the same.
    assert_eq!(
        cursor(&run(20, 5, "\x1b[2;3H\x1b[s\x1b[5;5H\x1b[u")),
        (2, 1)
    );
}

#[test]
fn autowrap_waits_for_the_next_character_and_can_be_turned_off() {
    // The 5th character leaves the cursor on the last column with the wrap pending.
    let grid = run(5, 3, "abcde");
    assert_eq!(cursor(&grid), (4, 0));
    assert_eq!(screen(&grid), rows(&["abcde", "", ""]));
    let grid = run(5, 3, "abcdef");
    assert_eq!(screen(&grid), rows(&["abcde", "f", ""]));
    assert_eq!(cursor(&grid), (1, 1));
    // A carriage return cancels the pending wrap.
    assert_eq!(screen(&run(5, 3, "abcde\rX")), rows(&["Xbcde", "", ""]));
    // DECAWM off: the last cell is overwritten.
    let grid = run(5, 3, "\x1b[?7labcdefg");
    assert_eq!(screen(&grid), rows(&["abcdg", "", ""]));
    assert_eq!(cursor(&grid), (4, 0));
    // And on again.
    assert_eq!(
        screen(&run(5, 3, "\x1b[?7l\x1b[?7habcdef")),
        rows(&["abcde", "f", ""])
    );
}

#[test]
fn tab_stops_every_eight_columns_set_clear_and_move() {
    assert_eq!(cursor(&run(40, 2, "\t")), (8, 0));
    assert_eq!(cursor(&run(40, 2, "ab\t")), (8, 0));
    assert_eq!(cursor(&run(40, 2, "\t\t\t")), (24, 0));
    assert_eq!(cursor(&run(20, 2, "\t\t\t")), (19, 0));
    // CHT and CBT.
    assert_eq!(cursor(&run(40, 2, "\x1b[2I")), (16, 0));
    assert_eq!(cursor(&run(40, 2, "\x1b[30G\x1b[Z")), (24, 0));
    assert_eq!(cursor(&run(40, 2, "\x1b[30G\x1b[2Z")), (16, 0));
    // HTS sets a stop, TBC 0 clears the one at the cursor, TBC 3 clears all.
    assert_eq!(cursor(&run(40, 2, "\x1b[4G\x1bH\x1b[1G\t")), (3, 0));
    assert_eq!(cursor(&run(40, 2, "\x1b[9G\x1b[0g\x1b[1G\t")), (16, 0));
    assert_eq!(cursor(&run(40, 2, "\x1b[3g\t")), (39, 0));
    assert_eq!(cursor(&run(40, 2, "\x1b[3g\x1b[6G\x1bH\x1b[1G\t")), (5, 0));
}

// ---- scrolling regions (vttest 1, 2) --------------------------------------------------

fn numbered(rows: u16) -> TermGrid {
    let mut grid = term(6, rows);
    for i in 1..=rows {
        grid.advance(format!("\x1b[{i};1H{i}").as_bytes());
    }
    grid
}

#[test]
fn decstbm_sets_the_region_and_homes_the_cursor() {
    let mut grid = numbered(8);
    grid.advance(b"\x1b[3;6r");
    assert_eq!(cursor(&grid), (0, 0));
    // LF at the bottom margin scrolls only the region.
    grid.advance(b"\x1b[6;1H\n");
    assert_eq!(
        screen(&grid),
        rows(&["1", "2", "4", "5", "6", "", "7", "8"])
    );
    assert_eq!(cursor(&grid), (0, 5));
    // LF outside the region just moves.
    grid.advance(b"\x1b[7;1H\n");
    assert_eq!(cursor(&grid), (0, 7));
    // RI at the top margin scrolls the region down.
    let mut grid = numbered(8);
    grid.advance(b"\x1b[3;6r\x1b[3;1H\x1bM");
    assert_eq!(
        screen(&grid),
        rows(&["1", "2", "", "3", "4", "5", "7", "8"])
    );
}

#[test]
fn a_region_that_is_invalid_is_ignored_and_reset_restores_the_screen() {
    let mut grid = numbered(6);
    grid.advance(b"\x1b[4;4r\x1b[6;1H\n");
    assert_eq!(screen(&grid)[0], "2", "a region of one line is ignored");
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;4r\x1b[r\x1b[6;1H\n");
    assert_eq!(screen(&grid)[0], "2");
}

#[test]
fn scroll_up_and_down_move_the_region_and_leave_the_cursor() {
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;5r\x1b[3;3H\x1b[2S");
    assert_eq!(screen(&grid), rows(&["1", "4", "5", "", "", "6"]));
    assert_eq!(cursor(&grid), (2, 2));
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;5r\x1b[1T");
    assert_eq!(screen(&grid), rows(&["1", "", "2", "3", "4", "6"]));
}

#[test]
fn origin_mode_makes_positions_relative_to_the_region() {
    let mut grid = numbered(8);
    grid.advance(b"\x1b[3;6r\x1b[?6h");
    assert_eq!(
        cursor(&grid),
        (0, 2),
        "enabling the mode homes inside the region"
    );
    grid.advance(b"\x1b[2;3HX");
    assert_eq!(screen(&grid)[3], "4 X");
    // The cursor cannot leave the region.
    grid.advance(b"\x1b[99;1H");
    assert_eq!(cursor(&grid), (0, 5));
    grid.advance(b"\x1b[?6l");
    assert_eq!(cursor(&grid), (0, 0));
}

#[test]
fn full_screen_scrolling_keeps_the_scrolled_lines_a_partial_region_drops_them() {
    let mut grid = term(6, 3);
    for i in 1..=5 {
        grid.advance(format!("{i}\r\n").as_bytes());
    }
    assert_eq!(grid.history_size(), 3);
    let mut grid = term(6, 5);
    grid.advance(b"\x1b[2;4r\x1b[4;1H");
    for _ in 0..10 {
        grid.advance(b"x\n");
    }
    assert_eq!(
        grid.history_size(),
        0,
        "a region below the top does not feed the scrollback"
    );
}

// ---- insert and delete (vttest 1, 8) --------------------------------------------------

#[test]
fn ich_dch_and_ech_shift_or_blank_within_the_line() {
    let at = |seq: &str| screen(&run(10, 2, &format!("abcdef\x1b[1;3H{seq}")));
    assert_eq!(at("\x1b[2@")[0], "ab  cdef");
    assert_eq!(at("\x1b[@")[0], "ab cdef");
    assert_eq!(at("\x1b[2P")[0], "abef");
    assert_eq!(at("\x1b[P")[0], "abdef");
    assert_eq!(at("\x1b[99P")[0], "ab");
    assert_eq!(at("\x1b[2X")[0], "ab  ef");
    assert_eq!(at("\x1b[99X")[0], "ab");
    // Characters pushed off the right edge are lost.
    assert_eq!(screen(&run(6, 1, "abcdef\x1b[1;2H\x1b[3@"))[0], "a   bc");
}

#[test]
fn el_and_ed_erase_the_parts_the_spec_names() {
    let base = "\x1b[1;1Hone\x1b[2;1Htwo\x1b[3;1Hthree\x1b[4;1Hfour\x1b[2;2H";
    let after = |seq: &str| screen(&run(8, 4, &format!("{base}{seq}")));
    assert_eq!(after("\x1b[K")[1], "t");
    assert_eq!(after("\x1b[0K")[1], "t");
    assert_eq!(after("\x1b[1K")[1], "  o");
    assert_eq!(after("\x1b[2K")[1], "");
    assert_eq!(after("\x1b[J"), rows(&["one", "t", "", ""]));
    assert_eq!(after("\x1b[1J"), rows(&["", "  o", "three", "four"]));
    assert_eq!(after("\x1b[2J"), rows(&["", "", "", ""]));
    // ED 2 leaves the cursor where it was.
    assert_eq!(cursor(&run(8, 4, &format!("{base}\x1b[2J"))), (1, 1));
}

#[test]
fn il_and_dl_work_inside_the_region_only() {
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;5r\x1b[3;1H\x1b[2L");
    assert_eq!(screen(&grid), rows(&["1", "2", "", "", "3", "6"]));
    assert_eq!(
        cursor(&grid),
        (0, 2),
        "the cursor is at the start of the line"
    );
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;5r\x1b[3;1H\x1b[2M");
    assert_eq!(screen(&grid), rows(&["1", "2", "5", "", "", "6"]));
    // Outside the region they do nothing.
    let mut grid = numbered(6);
    grid.advance(b"\x1b[2;5r\x1b[6;1H\x1b[L\x1b[M");
    assert_eq!(screen(&grid), rows(&["1", "2", "3", "4", "5", "6"]));
}

#[test]
fn insert_mode_pushes_characters_to_the_right() {
    assert_eq!(
        screen(&run(10, 2, "abcdef\x1b[1;3H\x1b[4hXY"))[0],
        "abXYcdef"
    );
    assert_eq!(
        screen(&run(10, 2, "abcdef\x1b[1;3H\x1b[4h\x1b[4lXY"))[0],
        "abXYef"
    );
}

#[test]
fn erasing_uses_the_current_background_colour() {
    let mut grid = run(6, 2, "\x1b[44m\x1b[1;3H\x1b[K");
    let runs = runs_of(&mut grid, 0);
    assert_eq!(runs.last().map(|r| r[2].clone()), Some(json!(PALETTE + 4)));
    let mut grid = run(6, 2, "\x1b[41m\x1b[2J");
    assert_eq!(runs_of(&mut grid, 1)[0][2], json!(PALETTE + 1));
}

#[test]
fn rep_repeats_the_last_printed_character() {
    assert_eq!(screen(&run(10, 1, "ab\x1b[3b"))[0], "abbbb");
    assert_eq!(screen(&run(10, 1, "\x1b[3b"))[0], "", "nothing to repeat");
}

#[test]
fn decaln_fills_the_screen_with_e() {
    let grid = run(4, 3, "\x1b#8");
    assert_eq!(screen(&grid), rows(&["EEEE", "EEEE", "EEEE"]));
}

// ---- character sets (vttest 3) --------------------------------------------------------

#[test]
fn dec_special_graphics_in_g0_and_g1() {
    assert_eq!(screen(&run(10, 1, "\x1b(0lqk\x1b(Bx"))[0], "┌─┐x");
    // G1 selected with SO, back with SI.
    assert_eq!(screen(&run(10, 1, "\x1b)0a\x0eqq\x0fq"))[0], "a──q");
    // The designation does not change what is already on the screen.
    assert_eq!(screen(&run(10, 1, "q\x1b(0q\x1b(Bq"))[0], "q─q");
}

#[test]
fn utf8_text_combining_marks_and_wide_characters() {
    let grid = run(10, 1, "é日本x");
    assert_eq!(screen(&grid)[0], "é日本x");
    assert_eq!(cursor(&grid), (6, 0), "two wide characters take four cells");
    // A combining accent sits on the character before it.
    let grid = run(10, 1, "e\u{301}x");
    assert_eq!(screen(&grid)[0], "e\u{301}x");
    assert_eq!(cursor(&grid), (2, 0));
    // A wide character that does not fit wraps whole.
    let grid = run(5, 2, "abcd日");
    assert_eq!(screen(&grid), rows(&["abcd", "日"]));
}

#[test]
fn invalid_and_split_utf8_never_break_the_stream() {
    let mut grid = term(10, 1);
    grid.advance(b"a\xffb");
    assert_eq!(screen(&grid)[0], "a\u{fffd}b");
    // A character split across two reads arrives whole.
    let mut grid = term(10, 1);
    let bytes = "perché".as_bytes();
    grid.advance(&bytes[..bytes.len() - 1]);
    grid.advance(&bytes[bytes.len() - 1..]);
    assert_eq!(screen(&grid)[0], "perché");
    // An overlong encoding and a lone continuation byte are replaced.
    let mut grid = term(10, 1);
    grid.advance(b"\xc0\xaf\x80z");
    assert!(screen(&grid)[0].ends_with('z'));
}

// ---- rendition (vttest 2) -------------------------------------------------------------

fn first_run(input: &str) -> Vec<Value> {
    let mut grid = run(20, 2, input);
    let runs = runs_of(&mut grid, 0);
    runs.first()
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default()
}

#[test]
fn sgr_attributes_set_and_reset_individually() {
    let flags = |sgr: &str| {
        first_run(&format!("\x1b[{sgr}mx"))
            .get(3)
            .and_then(Value::as_u64)
    };
    assert_eq!(flags("1"), Some(1)); // bold
    assert_eq!(flags("2"), Some(16)); // dim
    assert_eq!(flags("3"), Some(2)); // italic
    assert_eq!(flags("4"), Some(4)); // underline
    assert_eq!(flags("7"), Some(8)); // inverse
    assert_eq!(flags("8"), Some(128)); // hidden
    assert_eq!(flags("9"), Some(32)); // strikeout
    assert_eq!(flags("4:3"), Some(64)); // curly underline
    assert_eq!(flags("1;3;4"), Some(1 | 2 | 4));
    // Resets: 22 bold and dim, 23 italic, 24 underline, 27 inverse, 28 hidden, 29 strikeout.
    assert_eq!(flags("1;2;22"), Some(0));
    assert_eq!(flags("3;23"), Some(0));
    assert_eq!(flags("4;24"), Some(0));
    assert_eq!(flags("7;27"), Some(0));
    assert_eq!(flags("8;28"), Some(0));
    assert_eq!(flags("9;29"), Some(0));
    assert_eq!(flags("1;4;0"), Some(0));
    assert_eq!(flags(""), Some(0), "SGR with no parameter is a reset");
    // Blink has no rendition here, and does not disturb the others.
    assert_eq!(flags("5;1"), Some(1));
}

#[test]
fn sgr_colours_basic_bright_indexed_and_truecolour() {
    let fg = |sgr: &str| first_run(&format!("\x1b[{sgr}mx")).get(1).cloned();
    let bg = |sgr: &str| first_run(&format!("\x1b[{sgr}mx")).get(2).cloned();
    for i in 0..8 {
        assert_eq!(
            fg(&format!("{}", 30 + i)),
            Some(json!(PALETTE + i64::from(i)))
        );
        assert_eq!(
            bg(&format!("{}", 40 + i)),
            Some(json!(PALETTE + i64::from(i)))
        );
        assert_eq!(
            fg(&format!("{}", 90 + i)),
            Some(json!(PALETTE + 8 + i64::from(i)))
        );
        assert_eq!(
            bg(&format!("{}", 100 + i)),
            Some(json!(PALETTE + 8 + i64::from(i)))
        );
    }
    assert_eq!(fg("38;5;200"), Some(json!(PALETTE + 200)));
    assert_eq!(bg("48;5;17"), Some(json!(PALETTE + 17)));
    assert_eq!(fg("38;2;1;2;3"), Some(json!(0x010203)));
    assert_eq!(bg("48;2;255;128;0"), Some(json!(0xff8000)));
    // The colon form of ITU T.416, with and without the colour space slot.
    assert_eq!(fg("38:2::10:20:30"), Some(json!(0x0a141e)));
    assert_eq!(fg("38:2:10:20:30"), Some(json!(0x0a141e)));
    assert_eq!(fg("38:5:9"), Some(json!(PALETTE + 9)));
    // 39 and 49 restore the defaults.
    assert_eq!(fg("31;39"), Some(json!(-1)));
    assert_eq!(bg("41;49"), Some(json!(-1)));
    // Attributes and colours combine.
    let run = first_run("\x1b[1;33;44mx");
    assert_eq!(run[1], json!(PALETTE + 3));
    assert_eq!(run[2], json!(PALETTE + 4));
    assert_eq!(run[3], json!(1));
}

#[test]
fn a_changed_rendition_makes_a_new_run_and_equal_neighbours_share_one() {
    let mut grid = run(20, 1, "ab\x1b[1mcd\x1b[0mef");
    let runs = runs_of(&mut grid, 0);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[1][0], "cd");
    let mut grid = run(20, 1, "a\x1b[1m\x1b[22mb");
    assert_eq!(runs_of(&mut grid, 0).len(), 1);
}

// ---- modes ----------------------------------------------------------------------------

#[test]
fn the_cursor_can_be_hidden_and_shaped() {
    let mut grid = run(10, 2, "\x1b[?25l");
    let frame = grid.frame(1).unwrap_or(Value::Null);
    assert_eq!(frame["cursor"]["visible"], false);
    grid.advance(b"\x1b[?25h");
    assert_eq!(
        grid.frame(1).unwrap_or(Value::Null)["cursor"]["visible"],
        true
    );
    for (code, shape, blink) in [
        (0, "block", false),
        (1, "block", true),
        (2, "block", false),
        (3, "underline", true),
        (4, "underline", false),
        (5, "beam", true),
        (6, "beam", false),
    ] {
        let mut grid = run(10, 2, &format!("\x1b[{code} q"));
        let cursor = grid.frame(1).unwrap_or(Value::Null)["cursor"].clone();
        assert_eq!(cursor["shape"], shape, "DECSCUSR {code}");
        assert_eq!(cursor["blink"], blink, "DECSCUSR {code}");
    }
}

#[test]
fn the_alternate_screen_keeps_the_main_screen_and_the_cursor() {
    let mut grid = run(10, 3, "main\x1b[2;3H\x1b[?1049h");
    assert_eq!(
        screen(&grid),
        rows(&["", "", ""]),
        "the alternate screen starts blank"
    );
    grid.advance(b"alt\x1b[?1049l");
    assert_eq!(screen(&grid)[0], "main");
    assert_eq!(cursor(&grid), (2, 1), "the cursor comes back");
    // 1047 and 47 switch the buffer as well (47 without the cursor save).
    let grid = run(10, 3, "main\x1b[?47hx\x1b[?47l");
    assert_eq!(screen(&grid)[0], "main");
    let grid = run(10, 3, "main\x1b[?1047hx\x1b[?1047l");
    assert_eq!(screen(&grid)[0], "main");
    // The alternate screen has no scrollback.
    let mut grid = run(10, 3, "\x1b[?1049h");
    for _ in 0..10 {
        grid.advance(b"x\r\n");
    }
    assert_eq!(grid.history_size(), 0);
    assert_eq!(grid.modes() & 32, 32);
}

#[test]
fn modes_the_app_needs_are_reported_in_the_frame() {
    let grid = run(
        10,
        3,
        "\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?1006h\x1b[?1002h",
    );
    let bits = grid.modes();
    for (bit, name) in [
        (1, "application cursor keys"),
        (64, "application keypad"),
        (2, "bracketed paste"),
        (16, "focus events"),
        (8, "SGR mouse"),
        (4 | 128, "button-event mouse"),
    ] {
        assert_eq!(bits & bit, bit, "{name}");
    }
    let grid = run(10, 3, "\x1b[?1h\x1b[?1l\x1b=\x1b>");
    assert_eq!(grid.modes() & (1 | 64), 0);
}

#[test]
fn reverse_wraparound_and_other_unknown_modes_do_not_disturb_the_stream() {
    let grid = run(10, 2, "\x1b[?9999hok\x1b[?9999l");
    assert_eq!(screen(&grid)[0], "ok");
}

#[test]
fn ris_resets_everything_and_decstr_keeps_the_screen() {
    let mut grid = run(10, 3, "text\x1b[31m\x1b[2;3r\x1b[?1h\x1b[?7l\x1bc");
    assert_eq!(screen(&grid), rows(&["", "", ""]));
    assert_eq!(cursor(&grid), (0, 0));
    assert_eq!(grid.modes() & 1, 0);
    grid.advance(b"abcdefghijk");
    assert_eq!(screen(&grid)[1], "k", "autowrap is on again");
    let mut grid = run(10, 3, "text\x1b[31m\x1b[?1h\x1b[3;1H\x1b[!p");
    assert_eq!(screen(&grid)[0], "text", "a soft reset keeps the screen");
    assert_eq!(grid.modes() & 1, 0);
    grid.advance(b"x");
    assert_eq!(runs_of(&mut grid, 2)[0][1], json!(-1), "and the rendition");
}

// ---- reports (vttest 6) ---------------------------------------------------------------

#[test]
fn device_attributes_status_and_cursor_position_are_answered() {
    let mut grid = run(20, 10, "\x1b[c");
    assert!(replies(&mut grid).starts_with("\x1b[?"));
    let mut grid = run(20, 10, "\x1b[0c");
    assert!(replies(&mut grid).ends_with('c'));
    let mut grid = run(20, 10, "\x1b[>c");
    let secondary = replies(&mut grid);
    assert!(
        secondary.starts_with("\x1b[>") && secondary.ends_with('c'),
        "{secondary}"
    );
    let mut grid = run(20, 10, "\x1b[5n");
    assert_eq!(replies(&mut grid), "\x1b[0n");
    let mut grid = run(20, 10, "\x1b[4;7H\x1b[6n");
    assert_eq!(replies(&mut grid), "\x1b[4;7R");
    // Asked twice, answered twice, in order.
    let mut grid = run(20, 10, "\x1b[5n\x1b[1;1H\x1b[6n");
    assert_eq!(replies(&mut grid), "\x1b[0n\x1b[1;1R");
    // Nothing is answered until it is asked.
    let mut grid = run(20, 10, "plain text");
    assert!(replies(&mut grid).is_empty());
}

#[test]
fn window_operations_report_sizes_and_the_title_stack_works() {
    let mut grid = term(30, 10);
    grid.cell_size = (9, 18);
    grid.advance(b"\x1b[18t");
    assert_eq!(replies(&mut grid), "\x1b[8;10;30t");
    grid.advance(b"\x1b[14t");
    assert_eq!(replies(&mut grid), "\x1b[4;180;270t");
    grid.advance(b"\x1b[16t");
    assert_eq!(replies(&mut grid), "\x1b[6;18;9t");
    // The title stack (xterm 22/23): push, change, pop.
    let mut grid = run(10, 3, "\x1b]2;first\x07");
    assert_eq!(grid.frame(1).unwrap_or(Value::Null)["title"], "first");
    grid.advance(b"\x1b[22;0t\x1b]2;second\x07");
    assert_eq!(grid.frame(1).unwrap_or(Value::Null)["title"], "second");
    grid.advance(b"\x1b[23;0t");
    assert_eq!(grid.frame(1).unwrap_or(Value::Null)["title"], "first");
}

#[test]
fn titles_ends_of_string_and_what_aborts_them() {
    let title = |input: &str| {
        let mut grid = run(10, 2, input);
        grid.frame(1).unwrap_or(Value::Null)["title"].clone()
    };
    assert_eq!(title("\x1b]0;a title\x07"), "a title");
    assert_eq!(title("\x1b]2;st ended\x1b\\"), "st ended");
    assert_eq!(
        title("\x1b]1;icon\x07"),
        Value::Null.clone(),
        "icon names are not titles"
    );
    assert_eq!(
        title("\x1b]0;caf\u{e9} \u{65e5}\u{672c}\x07"),
        "caf\u{e9} \u{65e5}\u{672c}"
    );
    // CAN aborts the string; the text after it is printed.
    let grid = run(10, 2, "\x1b]0;lost\x18ok");
    assert_eq!(screen(&grid)[0], "ok");
}

#[test]
fn the_mode_report_decrqm_tells_the_state_of_a_mode() {
    let mut grid = run(10, 2, "\x1b[?25$p");
    assert_eq!(replies(&mut grid), "\x1b[?25;1$y");
    let mut grid = run(10, 2, "\x1b[?25l\x1b[?25$p");
    assert_eq!(replies(&mut grid), "\x1b[?25;2$y");
    let mut grid = run(10, 2, "\x1b[?9999$p");
    assert_eq!(replies(&mut grid), "\x1b[?9999;0$y");
}

// ---- robustness (vttest 11) -----------------------------------------------------------

#[test]
fn unknown_or_malformed_sequences_are_swallowed_and_text_continues() {
    for junk in [
        "\x1b[?99z",
        "\x1b[99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99;99m",
        "\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20;21;22;23;24;25;26;27;28;29;30;31;32H",
        "\x1b%G",
        "\x1b^private\x1b\\",
        "\x1b_application\x1b\\",
        "\x1bP1$rjunk\x1b\\",
        "\x1b[",
    ] {
        let mut grid = run(20, 3, junk);
        grid.advance(b"\x18xyz");
        assert!(
            screen(&grid)[0].ends_with("xyz"),
            "after {junk:?}: {:?}",
            screen(&grid)
        );
    }
}

#[test]
fn escape_inside_a_sequence_starts_a_new_one_and_can_aborts() {
    // ESC in the middle of a CSI abandons it and starts the next sequence.
    assert_eq!(cursor(&run(20, 5, "\x1b[3\x1b[2;2H")), (1, 1));
    // CAN and SUB abort a sequence and print nothing themselves.
    assert_eq!(screen(&run(20, 2, "\x1b[3\x18ab"))[0], "ab");
    assert_eq!(screen(&run(20, 2, "\x1b[3\x1aab"))[0], "ab");
    // C0 controls inside a CSI act at once; the sequence goes on.
    assert_eq!(cursor(&run(20, 5, "\x1b[2\n;3H")), (2, 1));
}

#[test]
fn a_very_long_line_and_binary_garbage_do_not_panic() {
    let mut grid = term(80, 24);
    grid.advance(&vec![b'x'; 200_000]);
    assert_eq!(screen(&grid).len(), 24);
    let garbage: Vec<u8> = (0..65_536u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    grid.advance(&garbage);
    assert_eq!(screen(&grid).len(), 24);
    // And the terminal still works.
    grid.advance(b"\x1bc\x1b[Hok");
    assert_eq!(screen(&grid)[0], "ok");
}

#[test]
fn a_one_cell_terminal_survives_everything() {
    // The engine panics on a wide character in one column: the grid is never narrower than two.
    let mut grid = term(1, 1);
    grid.advance(b"abc\r\n\x1b[5;5H\x1b[2J\x1b[L\x1b[M\x1b[@\x1b[P\x1b[S\x1b[T\x1b[r\n");
    grid.resize(1, 1);
    grid.advance("日\u{301}".as_bytes());
    assert_eq!(screen(&grid).len(), 1);
}

#[test]
fn csi_sequences_cut_by_the_end_of_a_read_are_not_seen_in_halves() {
    // DCH in halves, ED 1 in halves, an unrelated one in halves.
    let mut grid = term(10, 3);
    grid.advance(b"abcdef\x1b[1;3H\x1b[9");
    grid.advance(b"9P");
    assert_eq!(screen(&grid)[0], "ab");
    let mut grid = term(10, 3);
    grid.advance(b"x\x1b");
    grid.advance(b"[31mred");
    assert_eq!(screen(&grid)[0], "xred");
    assert_eq!(
        runs_of(&mut grid, 0).last().map(|r| r[1].clone()),
        Some(json!(PALETTE + 1))
    );
    // Byte by byte.
    let mut grid = term(10, 3);
    for b in b"abcdef\x1b[1;3H\x1b[99P".iter() {
        grid.advance(&[*b]);
    }
    assert_eq!(screen(&grid)[0], "ab");
}

// ---- resizing and reflow ---------------------------------------------------------------

#[test]
fn a_wrapped_line_reflows_when_the_width_changes() {
    let mut grid = run(10, 4, "abcdefghijklmnopqrstuvwxy");
    assert_eq!(
        screen(&grid),
        rows(&["abcdefghij", "klmnopqrst", "uvwxy", ""])
    );
    grid.resize(5, 6);
    // Nothing is lost: what no longer fits above the cursor is in the scrollback.
    assert_eq!(
        grid.text(),
        "abcdefghijklmnopqrstuvwxy",
        "still one line, wrapped"
    );
    assert_eq!(screen(&grid)[..3], rows(&["klmno", "pqrst", "uvwxy"]));
    assert_eq!(grid.history_size(), 2);
    grid.resize(25, 6);
    assert_eq!(grid.text(), "abcdefghijklmnopqrstuvwxy");
    // Lines ended with a newline stay apart.
    let mut grid = run(10, 4, "ab\r\ncd");
    grid.resize(40, 4);
    assert_eq!(screen(&grid)[..2], rows(&["ab", "cd"]));
}

#[test]
fn shrinking_the_height_pushes_lines_into_the_scrollback_and_growing_brings_them_back() {
    let mut grid = term(6, 4);
    for i in 1..=4 {
        grid.advance(format!("l{i}\r\n").as_bytes());
    }
    grid.advance(b"x");
    grid.resize(6, 2);
    assert!(grid.history_size() > 0);
    assert_eq!(screen(&grid).last().map(String::as_str), Some("x"));
    grid.resize(6, 4);
    assert_eq!(
        screen(&grid)[0],
        "l2",
        "the lines that went up come back at the top"
    );
}

#[test]
fn the_cursor_stays_on_its_text_through_a_resize() {
    let mut grid = run(10, 4, "abcdefghijklm");
    assert_eq!(cursor(&grid), (3, 1));
    grid.resize(5, 4);
    let (x, y) = cursor(&grid);
    assert_eq!(screen(&grid)[y].chars().nth(x.saturating_sub(1)), Some('m'));
}

#[test]
fn the_alternate_screen_is_not_reflowed() {
    let mut grid = run(10, 3, "\x1b[?1049habcdefghijkl");
    grid.resize(5, 3);
    grid.advance(b"\x1b[?1049l");
    assert_eq!(screen(&grid).len(), 3);
}

// ---- the bell and other C0 -------------------------------------------------------------

#[test]
fn the_bell_is_reported_once_per_frame() {
    let mut grid = run(10, 2, "\x07\x07");
    assert_eq!(grid.frame(1).unwrap_or(Value::Null)["bell"], true);
    assert!(grid.frame(1).is_none());
    // BEL inside an OSC ends the string and is not a bell.
    let mut grid = run(10, 2, "\x1b]0;t\x07");
    assert!(grid.frame(1).unwrap_or(Value::Null).get("bell").is_none());
}

// ---- what the engine does differently ----------------------------------------------------

/// Behaviour of the engine that differs from the specification (or from xterm). Asserted as it
/// is, so that a change of the engine is noticed. docs/conformance.md of the Terminal app lists
/// them.
#[test]
fn deviations() {
    // DECDWL and DECDHL (double-width and double-height lines, vttest 4) are not implemented:
    // the line is drawn at normal size.
    let grid = run(10, 3, "\x1b#6dw");
    assert_eq!(screen(&grid)[0], "dw");
    // DECCOLM (80/132 columns, vttest 1) does not resize the terminal: the window decides.
    let mut grid = run(40, 3, "\x1b[?3h");
    grid.advance(b"x");
    assert_eq!(grid.screen().len(), 3);
    assert_eq!(grid.cursor_cell().0, 1);
    // DECSCNM (reverse video of the whole screen) is not reported to the app.
    let mut grid = run(10, 3, "\x1b[?5h");
    assert!(
        grid.frame(1)
            .unwrap_or(Value::Null)
            .get("reverse")
            .is_none()
    );
    // CPR in origin mode reports the absolute row, where xterm reports it relative to the
    // region.
    let mut grid = run(20, 8, "\x1b[3;6r\x1b[?6h\x1b[2;3H\x1b[6n");
    assert_eq!(replies(&mut grid), "\x1b[4;3R");
    // Narrowing the window reflows with the blank rows below the cursor kept: the first lines
    // go to the scrollback although there is room.
    let mut grid = run(10, 4, "abcdefghijklmnopqrstuvwxy");
    grid.resize(5, 6);
    assert_eq!(grid.history_size(), 2);
    // Soft hyphens, bidirectional text and ligatures are the font's business.
}

/// The frame the Terminal app is tested against: the same literal is parsed by
/// `test/contract_test.dart` of the Terminal app (silica-os/terminal), so a change of the
/// message breaks one of the two.
#[test]
fn the_frame_contract_with_the_app() {
    let mut grid = term(20, 3);
    grid.advance(
        b"\x1b]7;file:///home/me/src\x07\x1b]133;A\x07$ \x1b]133;B\x07ls \x1b[1;31mred\x1b[0m ",
    );
    grid.advance(b"\x1b]8;;https://example.org\x07link\x1b]8;;\x07\x07\x1b]52;c;aGk=\x07");
    grid.select_start(0, SelectionType::Simple, 0, 0, false);
    grid.select_update(0, 3, 0, true);
    grid.advance(b"\x1b[?1000h\x1b[?1006h");
    let frame = grid.frame(5).unwrap_or(Value::Null);
    println!(
        "{}",
        serde_json::to_string_pretty(&frame).unwrap_or_default()
    );
    assert_eq!(frame, contract_frame());
}

pub(super) fn contract_frame() -> Value {
    json!({
        "id": 5, "cols": 20, "rows": 3, "full": true,
        "lines": [
            {"y": 0, "runs": [["$ ls ", -1, -1, 0, 5], ["red", PALETTE + 1, -1, 1, 3], [" ", -1, -1, 0, 1], ["link", -1, -1, 0, 4, "https://example.org"]]},
            {"y": 1, "runs": []},
            {"y": 2, "runs": []}
        ],
        "cursor": {"x": 13, "y": 0, "visible": true, "shape": "block", "blink": false},
        "modes": 2060,
        "scroll": {"offset": 0, "history": 0},
        "selection": [[0, 0, 3]],
        "marks": [0],
        "cwd": "/home/me/src",
        "bell": true,
        "clipboard": [{"set": "hi", "selection": false}]
    })
}

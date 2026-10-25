//! Game boards and pieces: chess boards from FEN, playing cards, dice faces.

use crate::modules::{Fail, Module};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "games",
    about: "draw chess positions, playing cards and dice",
    #[rustfmt::skip]
    examples: &[
        ("games", &[
            ("roll three dice and draw them", r#"dice(roll("3d6").rolls)"#),
            ("a royal flush", r#"cards(["10♠", "J♠", "Q♠", "K♠", "A♠"])"#),
            ("a chess board at the start of a game", "chess()"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{card as face, chess as board, die, side_by_side};
    use crate::modules::Fail;
    use crate::signature::{List, Or};
    use crate::value::Value;

    /// draw a chess board from FEN, the standard text notation for a position
    ///
    /// Only the first part, where the pieces are, is used. Shows the starting position without one
    #[builtin(examples = ["chess()", r#"chess("8/8/8/4k3/8/8/4P3/4K3 w - - 0 1")"#], see = ["card"])]
    fn chess(fen: Option<&str>) -> Result<String, Fail> {
        match fen {
            None => Ok(board("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")?),
            Some(fen) => board(fen.split_whitespace().next().unwrap_or("")).map_err(|e| Fail::Arg(0, e)),
        }
    }

    /// draw a playing card, written as its rank and suit like "A♠", "10h" or "qd"
    ///
    /// Suits are ♠♥♦♣ or the letters s h d c, and "?" draws the card's back
    #[builtin(examples = [r#"card("Q♥")"#], see = ["cards", "dice"])]
    fn card(c: &str) -> Result<String, Fail> {
        Ok(face(c).map_err(|e| Fail::Arg(0, e))?.join("\n"))
    }

    /// draw several playing cards side by side, written as for card
    #[builtin(examples = [r#"cards(["As", "Kh", "?"])"#], see = ["card"])]
    fn cards(hand: &List) -> Result<String, Fail> {
        let faces = hand
            .borrow()
            .iter()
            .map(|c| match c {
                Value::Str(c) => face(c),
                v => Err(format!("cards are strings like \"A♠\", got {}", v.type_name())),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Fail::Arg(0, e))?;
        Ok(side_by_side(&faces))
    }

    /// draw a die showing 1 to 6, or several side by side for a list
    #[builtin(examples = ["dice(5)", "dice([1, 2, 3])"], see = ["roll", "card"])]
    fn dice(faces: Or<i64, &List>) -> Result<String, Fail> {
        Ok(match faces {
            Or::A(n) => side_by_side(&[die(&Value::int(n))?]),
            Or::B(l) => side_by_side(&l.borrow().iter().map(die).collect::<Result<Vec<_>, _>>()?),
        })
    }
}

fn chess(board: &str) -> Result<String, String> {
    let ranks: Vec<&str> = board.split('/').collect();
    if ranks.len() != 8 {
        return Err(format!("a board has 8 ranks separated by /, got {}", ranks.len()));
    }
    let mut out = vec![];
    for (r, rank) in ranks.iter().enumerate() {
        let mut row = vec![];
        for c in rank.chars() {
            match c {
                '1'..='8' => row.extend(std::iter::repeat_n(None, c as usize - '0' as usize)),
                _ => row.push(Some(PIECES.iter().find(|p| p.0 == c).ok_or(format!("unknown piece {c:?}"))?.1)),
            }
        }
        if row.len() != 8 {
            return Err(format!("rank {} has {} squares, not 8", 8 - r, row.len()));
        }
        // a1 is dark: dark squares are where file + rank (both from 0 at a1) is even.
        let squares = row.iter().enumerate().map(|(f, p)| p.unwrap_or(if (f + 7 - r) % 2 == 0 { '·' } else { ' ' }).to_string());
        out.push(format!("{} {}", 8 - r, squares.collect::<Vec<_>>().join(" ")));
    }
    out.push("  a b c d e f g h".into());
    Ok(out.join("\n"))
}

fn card(c: &str) -> Result<Vec<String>, String> {
    if c.trim() == "?" {
        return Ok(["┌─────┐", "│░░░░░│", "│░░░░░│", "│░░░░░│", "└─────┘"].map(String::from).to_vec());
    }
    let c = c.trim();
    let suit = c.chars().last().ok_or("empty card")?;
    let rank = c[..c.len() - suit.len_utf8()].to_uppercase();
    let suit = match suit.to_ascii_lowercase() {
        's' | '♠' | '♤' => '♠',
        'h' | '♥' | '♡' => '♥',
        'd' | '♦' | '♢' => '♦',
        'c' | '♣' | '♧' => '♣',
        _ => return Err(format!("unknown suit {suit:?}; use ♠♥♦♣ or s h d c")),
    };
    let rank = match rank.as_str() {
        "T" => "10".to_string(),
        "A" | "J" | "Q" | "K" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10" => rank,
        _ => return Err(format!("unknown rank {rank:?}; use A, 2-10, J, Q or K")),
    };
    Ok(vec!["┌─────┐".into(), format!("│{rank:<5}│"), format!("│  {suit}  │"), format!("│{rank:>5}│"), "└─────┘".into()])
}

fn die(v: &Value) -> Result<Vec<String>, Fail> {
    let n = match v {
        Value::Int(n @ 1..=6, _) => *n as usize,
        _ => return Err(Fail::Arg(0, format!("die faces are 1 to 6, got {v}"))),
    };
    // Pip slots in reading order: top left, top right, middle left, center, middle right, bottom left, bottom right.
    let pips: &[usize] = [&[3][..], &[1, 5], &[0, 3, 6], &[0, 1, 5, 6], &[0, 1, 3, 5, 6], &[0, 1, 2, 4, 5, 6]][n - 1];
    let p = |i: usize| if pips.contains(&i) { '●' } else { ' ' };
    Ok(vec![
        "┌───────┐".into(),
        format!("│ {}   {} │", p(0), p(1)),
        format!("│ {} {} {} │", p(2), p(3), p(4)),
        format!("│ {}   {} │", p(5), p(6)),
        "└───────┘".into(),
    ])
}

/// Equal-height blocks of lines joined with one space between.
fn side_by_side(blocks: &[Vec<String>]) -> String {
    let h = blocks.first().map_or(0, Vec::len);
    (0..h).map(|r| blocks.iter().map(|b| b[r].as_str()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

const PIECES: &[(char, char)] =
    &[('K', '♔'), ('Q', '♕'), ('R', '♖'), ('B', '♗'), ('N', '♘'), ('P', '♙'), ('k', '♚'), ('q', '♛'), ('r', '♜'), ('b', '♝'), ('n', '♞'), ('p', '♟')];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn games() {
        let board = show("chess()");
        assert!(board.starts_with("8 ♜ ♞ ♝ ♛ ♚ ♝ ♞ ♜\n7 ♟"));
        assert!(board.contains("\n1 ♖ ♘ ♗ ♕ ♔ ♗ ♘ ♖\n  a b"));
        assert_eq!(show(r#"chess("8/8/8/8/8/8/8/K7")"#).lines().nth(7), Some("1 ♔   ·   ·   ·  "));
        assert!(try_eval(r#"chess("8/8")"#).is_err());
        assert!(try_eval(r#"chess("9/8/8/8/8/8/8/8")"#).is_err());
        assert_eq!(show(r#"card("10h")"#), "┌─────┐\n│10   │\n│  ♥  │\n│   10│\n└─────┘");
        assert!(try_eval(r#"card("1x")"#).is_err());
        assert_eq!(show(r#"cards(["As", "?"])"#).lines().nth(1), Some("│A    │ │░░░░░│"));
        assert_eq!(show("dice(1)").lines().nth(2), Some("│   ●   │"));
        assert_eq!(show("dice([6, 2])").lines().nth(2), Some("│ ●   ● │ │       │"));
        assert!(try_eval("dice(7)").is_err());
    }
}

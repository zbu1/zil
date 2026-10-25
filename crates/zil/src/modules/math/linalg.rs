//! Vectors and matrices as lists and lists of rows; ints and fractions stay exact, units carry through.

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, builtin, doc};
use crate::ops::binary;
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "linalg",
    about: "vectors and matrices: products, determinants, inverses and solving systems of equations",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Vectors and matrices: products, determinants, inverses, and solving systems of linear equations."),
        ("writing", "\
A vector is a list of numbers, like `[3, 4]`, and a matrix is a list of rows, like `[[1, 2], [3, 4]]` for the grid \
with 1 2 on top and 3 4 below."),
        ("good to know", "\
Given integers and fractions, every function here gives an exact answer."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("linalg", &[
            ("solve 2x + y = 3, x + 3y = 5", "solve([[2, 1], [1, 3]], [3, 5]).map(frac)"),
            ("exact inverse", "inv([[2, 1], [1, 1]])"),
            ("rotate the point (1, 0) by 90°", "matmul([[cos(90 deg), -sin(90 deg)], [sin(90 deg), cos(90 deg)]], [[1], [0]])"),
            ("the distance from 0 to (3 m, 4 m)", "norm([3 m, 4 m])"),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("vectors", &["dot", "cross", "norm"]),
        ("matrices", &["transpose", "matmul", "det", "inv", "solve"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("dot", "dot(a: list, b: list) -> num|quantity", "the dot product: multiply matching items of two equal-length vectors and add the results", &["dot([1, 2, 3], [4, 5, 6])"], &["cross", "norm"]),
    doc("cross", "cross(a: list, b: list) -> list", "the cross product of two 3D vectors: a vector at right angles to both", &["cross([1, 0, 0], [0, 1, 0])"], &["dot"]),
    doc("norm", "norm(v: list) -> float|quantity", "the length of a vector: the square root of the sum of its squared items", &["norm([3, 4])", "norm([1 m, 1 m])"], &["dot", "hypot"]),
    doc("transpose", "transpose(m: list) -> list", "a matrix with its rows and columns swapped", &["[[1, 2, 3], [4, 5, 6]].transpose"], &["matmul"]),
    doc("matmul", "matmul(a: list, b: list) -> list", "the matrix product of a and b, where a must have as many columns as b has rows", &["matmul([[1, 2], [3, 4]], [[5], [6]])"], &["transpose", "inv"]),
    doc("det", "det(m: list) -> num", "the determinant of a square matrix", &["det([[1, 2], [3, 4]])"], &["inv", "solve"])
        .detail("0 means it has no inverse"),
    doc("inv", "inv(m: list) -> list", "the inverse of a square matrix: the matrix that multiplies with it to give the identity", &["inv([[4, 7], [2, 6]])"], &["det", "solve"]),
    doc("solve", "solve(a: list, b: list) -> list", "solve linear equations: the list x for which matmul(a, x) gives b", &["solve([[2, 1], [1, 3]], [3, 5])", "solve([[1, 1, 1], [0, 2, 5], [2, 5, -1]], [6, -4, 27])"], &["inv", "det"])
        .detail("Each row of the square matrix a holds one equation's coefficients, and b the right-hand sides, so 2x + y = 3 and x + 3y = 5 is solve([[2, 1], [1, 3]], [3, 5])"),
];

fn op(o: BinOp, a: &Value, b: &Value) -> Result<Value, Fail> {
    Ok(binary(o, a, b)?)
}

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("dot", [List(a), List(b)]) => dot(&a.borrow(), &b.borrow())?,
        ("cross", [List(a), List(b)]) => {
            let (a, b) = (a.borrow(), b.borrow());
            if a.len() != 3 || b.len() != 3 {
                return Err("expected two vectors of 3 items".into());
            }
            let term = |i: usize, j: usize| op(BinOp::Sub, &op(BinOp::Mul, &a[i], &b[j])?, &op(BinOp::Mul, &a[j], &b[i])?);
            Value::list(vec![term(1, 2)?, term(2, 0)?, term(0, 1)?])
        }
        ("norm", [List(v)]) => {
            // Folding hypot keeps units, which sqrt(dot(v, v)) would not.
            let v = v.borrow().clone();
            let mut acc = it.call(&builtin("abs"), vec![v.first().ok_or("empty vector")?.clone()], span)?;
            for x in &v[1..] {
                acc = it.call(&builtin("hypot"), vec![acc, x.clone()], span)?;
            }
            acc
        }
        ("transpose", [m]) => {
            let m = matrix(m, 0)?;
            rows((0..m[0].len()).map(|j| m.iter().map(|r| r[j].clone()).collect()).collect())
        }
        ("matmul", [a, b]) => {
            let (a, b) = (matrix(a, 0)?, matrix(b, 1)?);
            if a[0].len() != b.len() {
                return Err(format!("can't multiply {}×{} by {}×{}", a.len(), a[0].len(), b.len(), b[0].len()).into());
            }
            let bt: Vec<Vec<Value>> = (0..b[0].len()).map(|j| b.iter().map(|r| r[j].clone()).collect()).collect();
            rows(a.iter().map(|r| bt.iter().map(|c| dot(r, c)).collect::<Result<_, _>>()).collect::<Result<_, _>>()?)
        }
        ("det", [m]) => eliminate(square(m)?)?.map_or(Value::int(0), |r| r.1),
        ("inv", [m]) => {
            let m = square(m)?;
            let n = m.len();
            let aug = m.into_iter().enumerate().map(|(i, mut r)| {
                r.extend((0..n).map(|j| Value::int((i == j).into())));
                r
            });
            let (done, _) = eliminate(aug.collect())?.ok_or("the matrix is singular")?;
            rows(done.into_iter().map(|r| r[n..].to_vec()).collect())
        }
        ("solve", [a, List(b)]) => {
            let a = square(a)?;
            let b = b.borrow();
            if b.len() != a.len() {
                return Err(Fail::Arg(1, format!("expected {} values, got {}", a.len(), b.len())));
            }
            let aug = a.into_iter().zip(b.iter()).map(|(mut r, v)| {
                r.push(v.clone());
                r
            });
            let (done, _) = eliminate(aug.collect())?.ok_or("no single solution: the matrix is singular")?;
            Value::list(done.into_iter().map(|r| r.last().unwrap().clone()).collect())
        }
        _ => return Err(Fail::BadArgs),
    })
}

type Matrix = Vec<Vec<Value>>;

fn dot(a: &[Value], b: &[Value]) -> Result<Value, Fail> {
    if a.len() != b.len() || a.is_empty() {
        return Err(format!("vectors must be the same non-zero length, got {} and {}", a.len(), b.len()).into());
    }
    let mut acc = op(BinOp::Mul, &a[0], &b[0])?;
    for (x, y) in a.iter().zip(b).skip(1) {
        acc = op(BinOp::Add, &acc, &op(BinOp::Mul, x, y)?)?;
    }
    Ok(acc)
}

fn rows(m: Vec<Vec<Value>>) -> Value {
    Value::list(m.into_iter().map(Value::list).collect())
}

/// A non-empty rectangular list of rows; argument `i` gets the blame.
fn matrix(v: &Value, i: usize) -> Result<Vec<Vec<Value>>, Fail> {
    let bad = || Fail::Arg(i, "expected a matrix: a list of rows, each a list of the same length".into());
    let Value::List(l) = v else { return Err(Fail::BadArgs) };
    let m: Vec<Vec<Value>> = l.borrow().iter().map(|r| if let Value::List(r) = r { Ok(r.borrow().clone()) } else { Err(bad()) }).collect::<Result<_, _>>()?;
    if m.is_empty() || m[0].is_empty() || m.iter().any(|r| r.len() != m[0].len()) {
        return Err(bad());
    }
    Ok(m)
}

fn square(v: &Value) -> Result<Vec<Vec<Value>>, Fail> {
    let m = matrix(v, 0)?;
    if m.len() != m[0].len() {
        return Err(Fail::Arg(0, format!("expected a square matrix, got {}×{}", m.len(), m[0].len())));
    }
    Ok(m)
}

/// Gauss-Jordan on the first n columns (n = row count): those become the identity, the rest is the answer.
/// Also returns the determinant; `None` when singular.
/// ponytail: a float pivot under 1e-12 counts as zero; exact values never get near that in practice.
fn eliminate(mut m: Matrix) -> Result<Option<(Matrix, Value)>, Fail> {
    let n = m.len();
    let size = |v: &Value| num(v).map_or(0.0, f64::abs);
    let mut det = Value::int(1);
    for c in 0..n {
        let p = (c..n).max_by(|&a, &b| size(&m[a][c]).total_cmp(&size(&m[b][c]))).unwrap();
        if size(&m[p][c]) < 1e-12 && !matches!(m[p][c], Value::Qty(..)) {
            return Ok(None);
        }
        if p != c {
            m.swap(p, c);
            det = op(BinOp::Sub, &Value::int(0), &det)?;
        }
        let pivot = m[c][c].clone();
        det = op(BinOp::Mul, &det, &pivot)?;
        m[c] = m[c].iter().map(|x| op(BinOp::Div, x, &pivot)).collect::<Result<_, _>>()?;
        for r in (0..n).filter(|&r| r != c) {
            let f = m[r][c].clone();
            if size(&f) == 0.0 && !matches!(f, Value::Qty(..)) {
                continue;
            }
            m[r] = m[r].iter().zip(&m[c]).map(|(x, y)| op(BinOp::Sub, x, &op(BinOp::Mul, &f, y)?)).collect::<Result<_, _>>()?;
        }
    }
    Ok(Some((m, det)))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn linalg() {
        assert_eq!(show("[dot([1, 2, 3], [4, 5, 6]), cross([1, 0, 0], [0, 1, 0]), norm([3, 4]), norm([3 m, 4 m])]"), "[32, [0, 0, 1], 5, 5 m]");
        assert_eq!(show("[[1, 2, 3], [4, 5, 6]].transpose"), "[[1, 4], [2, 5], [3, 6]]");
        assert_eq!(show("matmul([[1, 2], [3, 4]], [[5, 6], [7, 8]])"), "[[19, 22], [43, 50]]");
        assert_eq!(show("[det([[1, 2], [3, 4]]), det([[1, 2], [2, 4]]), det([[0, 1], [1, 0]])]"), "[-2, 0, -1]");
        assert_eq!(show("inv([[2, 1], [1, 1]])"), "[[1, -1], [-1, 2]]");
        assert_eq!(show("solve([[2, 1], [1, 3]], [3, 5]).map(frac)"), "[4/5, 7/5]");
        assert_eq!(show("solve([[1, 1, 1], [0, 2, 5], [2, 5, -1]], [6, -4, 27])"), "[5, 3, -2]");
        assert_eq!(show("matmul(inv([[4, 7], [2, 6]]), [[4, 7], [2, 6]])"), "[[1, 0], [0, 1]]");
        assert!(try_eval("inv([[1, 2], [2, 4]])").is_err());
        assert!(try_eval("det([[1, 2]])").is_err());
        assert!(try_eval("matmul([[1, 2]], [[1, 2]])").is_err());
    }
}

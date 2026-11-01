//! Vectors and matrices as lists and lists of rows; ints and fractions stay exact, units carry through.

use crate::ast::BinOp;
use crate::modules::{Fail, Module};
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
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("vectors", &["dot", "cross", "norm", "triple"]),
        ("matrices", &["transpose", "matmul", "det", "inv", "solve", "rref", "rank", "cofactor", "adjugate", "kron", "permanent"]),
        ("building", &["identity", "matrix", "reshape", "row", "col", "hcat", "vcat"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{dot as dot_of, eliminate, matrix, op, rows, rref as rref_of, square};
    use crate::ast::BinOp;
    use crate::ast::BinOp::Mul;
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::{Fail, builtin};
    use crate::signature::{As, List, Num, Or, Qty};
    use crate::value::Value;

    fn value(l: &List) -> Value {
        Value::List(l.clone())
    }

    /// the dot product: multiply matching items of two equal-length vectors and add the results
    #[builtin(examples = ["dot([1, 2, 3], [4, 5, 6])"], see = ["cross", "norm"])]
    fn dot(a: &List, b: &List) -> Result<As<Or<Num, Qty>>, Fail> {
        dot_of(&a.borrow(), &b.borrow()).map(As::from)
    }

    /// the cross product of two 3D vectors: a vector at right angles to both
    #[builtin(examples = ["cross([1, 0, 0], [0, 1, 0])"], see = ["dot"])]
    fn cross(a: &List, b: &List) -> Result<Vec<Value>, Fail> {
        let (a, b) = (a.borrow(), b.borrow());
        if a.len() != 3 || b.len() != 3 {
            return Err("expected two vectors of 3 items".into());
        }
        let term = |i: usize, j: usize| op(BinOp::Sub, &op(BinOp::Mul, &a[i], &b[j])?, &op(BinOp::Mul, &a[j], &b[i])?);
        Ok(vec![term(1, 2)?, term(2, 0)?, term(0, 1)?])
    }

    /// the length of a vector: the square root of the sum of its squared items
    #[builtin(examples = ["norm([3, 4])", "norm([1 m, 1 m])"], see = ["dot", "hypot"])]
    fn norm(it: &mut Interp, v: &List, span: &Span) -> Result<As<Or<f64, Qty>>, Fail> {
        // Folding hypot keeps units, which sqrt(dot(v, v)) would not.
        let v = v.borrow().clone();
        let mut acc = it.call(&builtin("abs"), vec![v.first().ok_or("empty vector")?.clone()], span)?;
        for x in &v[1..] {
            acc = it.call(&builtin("hypot"), vec![acc, x.clone()], span)?;
        }
        Ok(acc.into())
    }

    /// a matrix with its rows and columns swapped
    #[builtin(examples = ["[[1, 2, 3], [4, 5, 6]].transpose"], see = ["matmul"])]
    fn transpose(m: &List) -> Result<As<Vec<Value>>, Fail> {
        let m = matrix(&value(m), 0)?;
        Ok(rows((0..m[0].len()).map(|j| m.iter().map(|r| r[j].clone()).collect()).collect()).into())
    }

    /// the matrix product of a and b, where a must have as many columns as b has rows
    #[builtin(examples = ["matmul([[1, 2], [3, 4]], [[5], [6]])"], see = ["transpose", "inv"])]
    fn matmul(a: &List, b: &List) -> Result<As<Vec<Value>>, Fail> {
        let (a, b) = (matrix(&value(a), 0)?, matrix(&value(b), 1)?);
        if a[0].len() != b.len() {
            return Err(format!("can't multiply {}×{} by {}×{}", a.len(), a[0].len(), b.len(), b[0].len()).into());
        }
        let bt: Vec<Vec<Value>> = (0..b[0].len()).map(|j| b.iter().map(|r| r[j].clone()).collect()).collect();
        Ok(rows(a.iter().map(|r| bt.iter().map(|c| dot_of(r, c)).collect::<Result<_, _>>()).collect::<Result<_, _>>()?).into())
    }

    /// the determinant of a square matrix
    ///
    /// 0 means it has no inverse
    #[builtin(examples = ["det([[1, 2], [3, 4]])"], see = ["inv", "solve"])]
    fn det(m: &List) -> Result<As<Num>, Fail> {
        Ok(eliminate(square(&value(m))?)?.map_or(Value::int(0), |r| r.1).into())
    }

    /// the inverse of a square matrix: the matrix that multiplies with it to give the identity
    #[builtin(examples = ["inv([[4, 7], [2, 6]])"], see = ["det", "solve"])]
    fn inv(m: &List) -> Result<As<Vec<Value>>, Fail> {
        let m = square(&value(m))?;
        let n = m.len();
        let aug = m.into_iter().enumerate().map(|(i, mut r)| {
            r.extend((0..n).map(|j| Value::int((i == j).into())));
            r
        });
        let (done, _) = eliminate(aug.collect())?.ok_or("the matrix is singular")?;
        Ok(rows(done.into_iter().map(|r| r[n..].to_vec()).collect()).into())
    }

    /// the n by n identity matrix: 1 down the diagonal and 0 elsewhere
    #[builtin(examples = ["identity(3)"], see = ["matrix"])]
    fn identity(n: i64) -> Result<As<Vec<Value>>, Fail> {
        let n = usize::try_from(n).ok().filter(|&n| n > 0 && n <= 1000).ok_or("n must be from 1 to 1000")?;
        Ok(rows((0..n).map(|i| (0..n).map(|j| Value::int((i == j).into())).collect()).collect()).into())
    }

    /// a matrix of the given number of rows and columns, filled with fill (0 if left out)
    #[builtin(name = "matrix", examples = ["matrix(2, 3)", "matrix(2, 2, 7)"], see = ["identity", "reshape"])]
    fn filled(r: i64, c: i64, fill: Option<Num>) -> Result<As<Vec<Value>>, Fail> {
        let dim = |n: i64| usize::try_from(n).ok().filter(|&n| n > 0 && n <= 10_000);
        let (r, c) = dim(r).zip(dim(c)).filter(|(r, c)| r * c <= 1_000_000).ok_or("rows and columns must be positive, up to a million cells")?;
        let fill = fill.map_or(Value::int(0), |f| f.0);
        Ok(rows(vec![vec![fill; c]; r]).into())
    }

    /// the items of xs laid out as a matrix of the given rows and columns, filling row by row
    #[builtin(examples = ["reshape([1, 2, 3, 4, 5, 6], 2, 3)"], see = ["matrix", "transpose"])]
    fn reshape(xs: &List, r: i64, c: i64) -> Result<As<Vec<Value>>, Fail> {
        let xs = xs.borrow();
        let (r, c) = (usize::try_from(r).unwrap_or(0), usize::try_from(c).unwrap_or(0));
        if r == 0 || c == 0 || r.checked_mul(c) != Some(xs.len()) {
            return Err(format!("can not lay {} items out in {r} rows of {c}", xs.len()).into());
        }
        Ok(rows(xs.chunks(c).map(<[Value]>::to_vec).collect()).into())
    }

    /// row i of matrix m, counting from 0
    #[builtin(examples = ["row([[1, 2], [3, 4]], 1)"], see = ["col"])]
    fn row(m: &List, i: i64) -> Result<Vec<Value>, Fail> {
        let m = matrix(&value(m), 0)?;
        usize::try_from(i).ok().and_then(|i| m.get(i)).cloned().ok_or_else(|| Fail::Arg(1, format!("the matrix has {} rows", m.len())))
    }

    /// column j of matrix m, counting from 0
    #[builtin(examples = ["col([[1, 2], [3, 4]], 1)"], see = ["row"])]
    fn col(m: &List, j: i64) -> Result<Vec<Value>, Fail> {
        let m = matrix(&value(m), 0)?;
        let j = usize::try_from(j).ok().filter(|&j| j < m[0].len()).ok_or_else(|| Fail::Arg(1, format!("the matrix has {} columns", m[0].len())))?;
        Ok(m.iter().map(|r| r[j].clone()).collect())
    }

    /// two matrices side by side, which need the same number of rows
    #[builtin(examples = ["hcat([[1, 2], [3, 4]], [[5], [6]])"], see = ["vcat"])]
    fn hcat(a: &List, b: &List) -> Result<As<Vec<Value>>, Fail> {
        let (a, b) = (matrix(&value(a), 0)?, matrix(&value(b), 1)?);
        if a.len() != b.len() {
            return Err(format!("can not join {} rows to {} rows side by side", a.len(), b.len()).into());
        }
        Ok(rows(
            a.into_iter()
                .zip(b)
                .map(|(mut x, y)| {
                    x.extend(y);
                    x
                })
                .collect(),
        )
        .into())
    }

    /// two matrices one above the other, which need the same number of columns
    #[builtin(examples = ["vcat([[1, 2]], [[3, 4]])"], see = ["hcat"])]
    fn vcat(a: &List, b: &List) -> Result<As<Vec<Value>>, Fail> {
        let (mut a, b) = (matrix(&value(a), 0)?, matrix(&value(b), 1)?);
        if a[0].len() != b[0].len() {
            return Err(format!("can not stack {} columns on {} columns", a[0].len(), b[0].len()).into());
        }
        a.extend(b);
        Ok(rows(a).into())
    }

    /// the scalar triple product of three 3D vectors, the signed volume of the box they span
    #[builtin(examples = ["triple([1, 0, 0], [0, 1, 0], [0, 0, 1])"], see = ["dot", "cross"])]
    fn triple(a: &List, b: &List, c: &List) -> Result<As<Or<Num, Qty>>, Fail> {
        let x = cross(b, c)?;
        dot_of(&a.borrow(), &x).map(As::from)
    }

    /// the reduced row echelon form of a matrix: the simplest matrix its row operations can reach
    #[builtin(examples = ["rref([[1, 2, 3], [4, 5, 6], [7, 8, 9]])"], see = ["rank", "solve"])]
    fn rref(m: &List) -> Result<As<Vec<Value>>, Fail> {
        Ok(rows(rref_of(matrix(&value(m), 0)?)?).into())
    }

    /// the rank of a matrix: how many of its rows are independent of the others
    #[builtin(examples = ["rank([[1, 2], [2, 4]])", "rank([[1, 0], [0, 1]])"], see = ["rref", "det"])]
    fn rank(m: &List) -> Result<i64, Fail> {
        Ok(rref_of(matrix(&value(m), 0)?)?.iter().filter(|r| r.iter().any(|v| !super::is_zero(v))).count() as i64)
    }

    /// the cofactor of m at row i and column j: the determinant of what is left without them, signed
    #[builtin(examples = ["cofactor([[1, 2], [3, 4]], 0, 1)"], see = ["adjugate", "det"])]
    fn cofactor(m: &List, i: i64, j: i64) -> Result<As<Num>, Fail> {
        let m = square(&value(m))?;
        let (i, j) = (usize::try_from(i).ok().filter(|&i| i < m.len()), usize::try_from(j).ok().filter(|&j| j < m.len()));
        let (i, j) = i.zip(j).ok_or_else(|| format!("row and column must be from 0 to {}", m.len() - 1))?;
        Ok(super::cofactor(&m, i, j)?.into())
    }

    /// the adjugate of a square matrix: the transposed cofactors, which divided by the determinant give the inverse
    #[builtin(examples = ["adjugate([[1, 2], [3, 4]])"], see = ["cofactor", "inv"])]
    fn adjugate(m: &List) -> Result<As<Vec<Value>>, Fail> {
        let m = square(&value(m))?;
        let n = m.len();
        Ok(rows((0..n).map(|i| (0..n).map(|j| super::cofactor(&m, j, i)).collect::<Result<_, _>>()).collect::<Result<_, _>>()?).into())
    }

    /// the Kronecker product of two matrices: every item of a times the whole of b, in blocks
    #[builtin(examples = ["kron([[1, 2], [3, 4]], [[1, 1], [1, 1]])"], see = ["matmul"])]
    fn kron(a: &List, b: &List) -> Result<As<Vec<Value>>, Fail> {
        let (a, b) = (matrix(&value(a), 0)?, matrix(&value(b), 1)?);
        let mut out = vec![];
        for ra in &a {
            for rb in &b {
                out.push(ra.iter().flat_map(|x| rb.iter().map(move |y| op(Mul, x, y))).collect::<Result<_, _>>()?);
            }
        }
        Ok(rows(out).into())
    }

    /// the permanent of a square matrix: the determinant without the alternating signs
    #[builtin(examples = ["permanent([[1, 2], [3, 4]])"], see = ["det"])]
    fn permanent(m: &List) -> Result<As<Num>, Fail> {
        let m = square(&value(m))?;
        if m.len() > 10 {
            return Err("the permanent is only computed up to 10x10".into());
        }
        Ok(super::permanent(&m, 0, &mut vec![false; m.len()])?.into())
    }

    /// solve linear equations: the list x for which matmul(a, x) gives b
    ///
    /// Each row of the square matrix a holds one equation's coefficients, and b the right-hand sides, so 2x + y = 3 and x + 3y = 5 is solve([[2, 1], [1, 3]], [3, 5])
    #[builtin(examples = ["solve([[2, 1], [1, 3]], [3, 5])", "solve([[1, 1, 1], [0, 2, 5], [2, 5, -1]], [6, -4, 27])"], see = ["inv", "det"])]
    fn solve(a: &List, b: &List) -> Result<Vec<Value>, Fail> {
        let a = square(&value(a))?;
        let b = b.borrow();
        if b.len() != a.len() {
            return Err(Fail::Arg(1, format!("expected {} values, got {}", a.len(), b.len())));
        }
        let aug = a.into_iter().zip(b.iter()).map(|(mut r, v)| {
            r.push(v.clone());
            r
        });
        let (done, _) = eliminate(aug.collect())?.ok_or("no single solution: the matrix is singular")?;
        Ok(done.into_iter().map(|mut r| r.pop().unwrap_or(Value::Nil)).collect())
    }
}

fn op(o: BinOp, a: &Value, b: &Value) -> Result<Value, Fail> {
    Ok(binary(o, a, b)?)
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

fn is_zero(v: &Value) -> bool {
    num(v).is_some_and(|x| x.abs() < 1e-12)
}

/// ponytail: float pivots under 1e-12 count as zero, like `eliminate`.
fn rref(mut m: Matrix) -> Result<Matrix, Fail> {
    let (rows, cols) = (m.len(), m[0].len());
    let mut r = 0;
    for c in 0..cols {
        let Some(p) = (r..rows).find(|&i| !is_zero(&m[i][c])) else { continue };
        m.swap(p, r);
        let pivot = m[r][c].clone();
        m[r] = m[r].iter().map(|x| op(BinOp::Div, x, &pivot)).collect::<Result<_, _>>()?;
        for i in (0..rows).filter(|&i| i != r) {
            let f = m[i][c].clone();
            m[i] = m[i].iter().zip(&m[r]).map(|(x, y)| op(BinOp::Sub, x, &op(BinOp::Mul, &f, y)?)).collect::<Result<_, _>>()?;
        }
        r += 1;
        if r == rows {
            break;
        }
    }
    Ok(m)
}

fn minor(m: &Matrix, i: usize, j: usize) -> Matrix {
    m.iter().enumerate().filter(|&(r, _)| r != i).map(|(_, row)| row.iter().enumerate().filter(|&(c, _)| c != j).map(|(_, v)| v.clone()).collect()).collect()
}

fn cofactor(m: &Matrix, i: usize, j: usize) -> Result<Value, Fail> {
    let d = if m.len() == 1 { Value::int(1) } else { eliminate(minor(m, i, j))?.map_or(Value::int(0), |r| r.1) };
    if (i + j) % 2 == 1 { op(BinOp::Sub, &Value::int(0), &d) } else { Ok(d) }
}

fn permanent(m: &Matrix, row: usize, used: &mut Vec<bool>) -> Result<Value, Fail> {
    if row == m.len() {
        return Ok(Value::int(1));
    }
    let mut total = Value::int(0);
    for c in 0..m.len() {
        if !used[c] {
            used[c] = true;
            total = op(BinOp::Add, &total, &op(BinOp::Mul, &m[row][c], &permanent(m, row + 1, used)?)?)?;
            used[c] = false;
        }
    }
    Ok(total)
}

/// Gauss-Jordan on the first n columns (n = row count): those become the identity, the rest is the answer.
/// Also returns the determinant; `None` when singular.
/// ponytail: a float pivot under 1e-12 counts as zero; exact values never get near that in practice.
fn eliminate(mut m: Matrix) -> Result<Option<(Matrix, Value)>, Fail> {
    let n = m.len();
    let size = |v: &Value| num(v).map_or(0.0, f64::abs);
    let mut det = Value::int(1);
    for c in 0..n {
        let p = (c..n).max_by(|&a, &b| size(&m[a][c]).total_cmp(&size(&m[b][c]))).unwrap_or(c);
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
        assert_eq!(
            show("[identity(2), matrix(2, 2, 7), reshape([1, 2, 3, 4, 5, 6], 2, 3), row([[1, 2], [3, 4]], 1), col([[1, 2], [3, 4]], 1)]"),
            "[[[1, 0], [0, 1]], [[7, 7], [7, 7]], [[1, 2, 3], [4, 5, 6]], [3, 4], [2, 4]]"
        );
        assert_eq!(
            show("[hcat([[1, 2], [3, 4]], [[5], [6]]), vcat([[1, 2]], [[3, 4]]), triple([1, 0, 0], [0, 1, 0], [0, 0, 1])]"),
            "[[[1, 2, 5], [3, 4, 6]], [[1, 2], [3, 4]], 1]"
        );
        assert_eq!(
            show("[rank([[1, 2], [2, 4]]), rank([[1, 0], [0, 1]]), rref([[1, 2, 3], [4, 5, 6], [7, 8, 9]])]"),
            "[1, 2, [[1, 0, -1], [0, 1, 2], [0, 0, 0]]]"
        );
        assert_eq!(
            show("[cofactor([[1, 2], [3, 4]], 0, 1), adjugate([[1, 2], [3, 4]]), permanent([[1, 2], [3, 4]]), kron([[1, 2]], [[1], [1]])]"),
            "[-3, [[4, -2], [-3, 1]], 10, [[1, 2], [1, 2]]]"
        );
        assert!(try_eval("inv([[1, 2], [2, 4]])").is_err());
        assert!(try_eval("det([[1, 2]])").is_err());
        assert!(try_eval("matmul([[1, 2]], [[1, 2]])").is_err());
    }
}

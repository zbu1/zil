//! Hardware rules of thumb: wire gauges, drill bits, RAID capacity and memory latency.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "hardware",
    about: "wire gauges, drill bits, RAID capacity and memory timings",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Quick calculations for building and sizing hardware: how thick a wire or drill bit is, which bit to drill for a screw thread, how much space a RAID array leaves you, and how \
long memory takes to answer."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("hardware", &[
            ("how thick is 12 gauge wire?", "awg(12)"),
            ("the size of a number, letter or fraction drill bit", r##"drill_bit("#7")"##),
            ("which bit to drill before tapping an M6 thread", r#"tap_drill("M6")"#),
            ("usable space of 4 disks in RAID 5", "raid(5, 4 TB, 4)"),
            ("how long DDR5-6000 CL30 takes to answer", "ram_latency(6000, 30)"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{DRILLS, THREADS};
    use crate::modules::Fail;
    use crate::modules::units::terms;
    use crate::signature::{As, Qty};
    use crate::value::Value;

    /// the diameter of American Wire Gauge number gauge, in mm
    ///
    /// Gauges 0 to 40 are accepted, a larger number is a thinner wire
    #[builtin(examples = ["awg(12)", "awg(24) to in"], see = ["raid"])]
    fn awg(gauge: i64) -> Result<As<Qty>, Fail> {
        if !(0..=40).contains(&gauge) {
            return Err(Fail::Arg(0, "gauge must be from 0 to 40".into()));
        }
        Ok(Value::qty(0.127 * 92f64.powf((36 - gauge) as f64 / 39.0), terms("mm")?).into())
    }

    /// the diameter of a drill bit written as a number like "#7", a letter like "F" or a fraction of an inch like "1/4", in mm
    ///
    /// Numbers run from #1 to #80, a larger number is a smaller bit, and letters from A to Z are bigger than #1
    #[builtin(examples = [r##"drill_bit("#7")"##, r#"drill_bit("F")"#, r#"drill_bit("1/4") to in"#], see = ["tap_drill", "awg"])]
    fn drill_bit(size: &str) -> Result<As<Qty>, Fail> {
        let key = size.trim().to_uppercase();
        let inches = if let Some((a, b)) = key.split_once('/') {
            let (a, b) = (a.parse::<f64>().ok(), b.parse::<f64>().ok());
            a.zip(b).filter(|(_, b)| *b != 0.0).map(|(a, b)| a / b)
        } else {
            DRILLS.iter().find(|d| d.0 == key.trim_start_matches('#')).map(|d| d.1)
        };
        let inches = inches.ok_or_else(|| {
            Fail::Arg(
                0,
                format!(
                    "no drill bit {size:?}
note: try a number like \"#7\", a letter like \"F\" or a fraction like \"1/4\""
                ),
            )
        })?;
        Ok(Value::qty(inches * 25.4, terms("mm")?).into())
    }

    /// the drill bit to use before cutting a metric thread like "M6", as a diameter in mm
    ///
    /// It is the thread's size minus its pitch, for the standard coarse pitch, or "M8x1" for a fine one. Sizes M2 to M24 are known
    #[builtin(examples = [r#"tap_drill("M6")"#, r#"tap_drill("M8x1")"#], see = ["drill_bit"])]
    fn tap_drill(thread: &str) -> Result<As<Qty>, Fail> {
        let t = thread.trim().to_lowercase();
        let (size, fine) = match t.strip_prefix('m').map(|r| r.split_once('x')) {
            Some(Some((d, p))) => (d.parse::<f64>().ok(), p.parse::<f64>().ok()),
            Some(None) => (t[1..].parse::<f64>().ok(), None),
            None => (None, None),
        };
        let bad = || {
            Fail::Arg(
                0,
                format!(
                    "unknown thread {thread:?}
note: write a metric thread like \"M6\" or \"M8x1\", from M2 to M24"
                ),
            )
        };
        let d = size.ok_or_else(bad)?;
        let pitch = fine.or_else(|| THREADS.iter().find(|r| r.0 == d).map(|r| r.1)).ok_or_else(bad)?;
        Ok(Value::qty(d - pitch, terms("mm")?).into())
    }

    /// the usable capacity of n disks of size disk in a RAID array of the given level
    ///
    /// Levels 0, 1, 5, 6 and 10 are supported. RAID 1 mirrors every disk, and RAID 10 mirrors pairs, so it needs an even number of disks
    #[builtin(examples = ["raid(5, 4 TB, 4)", "raid(10, 2 TB, 6)"], see = ["awg"])]
    fn raid(level: i64, disk: Qty, n: i64) -> Result<As<Qty>, Fail> {
        let (min, data) = match level {
            0 => (1, n),
            1 => (2, 1),
            5 => (3, n - 1),
            6 => (4, n - 2),
            10 => (4, n / 2),
            _ => return Err(Fail::Arg(0, "level must be 0, 1, 5, 6 or 10".into())),
        };
        if n < min || (level == 10 && n % 2 != 0) {
            return Err(Fail::Arg(2, format!("RAID {level} needs {}{min} disks", if level == 10 { "an even number, at least " } else { "at least " })));
        }
        let (x, u) = disk.get();
        Ok(Value::Qty(x * data as f64, u.clone()).into())
    }

    /// the true latency of memory in ns, from its transfer rate in MT/s and CAS latency cl
    ///
    /// Memory does two transfers per clock, so the clock is half the rate
    #[builtin(examples = ["ram_latency(6000, 30)", "ram_latency(3200, 16)"])]
    fn ram_latency(rate: f64, cl: f64) -> Result<As<Qty>, Fail> {
        if rate <= 0.0 {
            return Err(Fail::Arg(0, "rate must be positive".into()));
        }
        Ok(Value::qty(cl * 2000.0 / rate, terms("ns")?).into())
    }
}

/// Standard coarse metric pitches, (diameter in mm, pitch in mm).
const THREADS: &[(f64, f64)] = &[
    (2.0, 0.4),
    (2.5, 0.45),
    (3.0, 0.5),
    (4.0, 0.7),
    (5.0, 0.8),
    (6.0, 1.0),
    (8.0, 1.25),
    (10.0, 1.5),
    (12.0, 1.75),
    (14.0, 2.0),
    (16.0, 2.0),
    (18.0, 2.5),
    (20.0, 2.5),
    (22.0, 2.5),
    (24.0, 3.0),
];

/// Number (1-80) and letter (A-Z) drill diameters in inches.
#[rustfmt::skip]
const DRILLS: &[(&str, f64)] = &[
    ("1", 0.2280), ("2", 0.2210), ("3", 0.2130), ("4", 0.2090), ("5", 0.2055), ("6", 0.2040), ("7", 0.2010), ("8", 0.1990), ("9", 0.1960), ("10", 0.1935),
    ("11", 0.1910), ("12", 0.1890), ("13", 0.1850), ("14", 0.1820), ("15", 0.1800), ("16", 0.1770), ("17", 0.1730), ("18", 0.1695), ("19", 0.1660), ("20", 0.1610),
    ("21", 0.1590), ("22", 0.1570), ("23", 0.1540), ("24", 0.1520), ("25", 0.1495), ("26", 0.1470), ("27", 0.1440), ("28", 0.1405), ("29", 0.1360), ("30", 0.1285),
    ("31", 0.1200), ("32", 0.1160), ("33", 0.1130), ("34", 0.1110), ("35", 0.1100), ("36", 0.1065), ("37", 0.1040), ("38", 0.1015), ("39", 0.0995), ("40", 0.0980),
    ("41", 0.0960), ("42", 0.0935), ("43", 0.0890), ("44", 0.0860), ("45", 0.0820), ("46", 0.0810), ("47", 0.0785), ("48", 0.0760), ("49", 0.0730), ("50", 0.0700),
    ("51", 0.0670), ("52", 0.0635), ("53", 0.0595), ("54", 0.0550), ("55", 0.0520), ("56", 0.0465), ("57", 0.0430), ("58", 0.0420), ("59", 0.0410), ("60", 0.0400),
    ("61", 0.0390), ("62", 0.0380), ("63", 0.0370), ("64", 0.0360), ("65", 0.0350), ("66", 0.0330), ("67", 0.0320), ("68", 0.0310), ("69", 0.0292), ("70", 0.0280),
    ("71", 0.0260), ("72", 0.0250), ("73", 0.0240), ("74", 0.0225), ("75", 0.0210), ("76", 0.0200), ("77", 0.0180), ("78", 0.0160), ("79", 0.0145), ("80", 0.0135),
    ("A", 0.2340), ("B", 0.2380), ("C", 0.2420), ("D", 0.2460), ("E", 0.2500), ("F", 0.2570), ("G", 0.2610), ("H", 0.2660), ("I", 0.2720), ("J", 0.2770),
    ("K", 0.2810), ("L", 0.2900), ("M", 0.2950), ("N", 0.3020), ("O", 0.3160), ("P", 0.3230), ("Q", 0.3320), ("R", 0.3390), ("S", 0.3480), ("T", 0.3580),
    ("U", 0.3680), ("V", 0.3770), ("W", 0.3860), ("X", 0.3970), ("Y", 0.4040), ("Z", 0.4130),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn hardware() {
        assert_eq!(show("awg(12).round(3)"), "2.053 mm");
        assert_eq!(show("raid(5, 4 TB, 4)"), "12 TB");
        assert_eq!(show("raid(10, 2 TB, 6)"), "6 TB");
        assert_eq!(show("ram_latency(6000, 30)"), "10 ns");
        assert_eq!(show(r##"drill_bit("#7").round(2)"##), "5.11 mm");
        assert_eq!(show(r#"drill_bit("1/4")"#), "6.35 mm");
        assert_eq!(show(r#"drill_bit("f").round(2)"#), "6.53 mm");
        assert_eq!(show(r#"tap_drill("M6")"#), "5 mm");
        assert_eq!(show(r#"tap_drill("M8x1")"#), "7 mm");
        assert!(try_eval(r##"drill_bit("#99")"##).is_err());
        assert!(try_eval(r#"tap_drill("M7")"#).is_err());
        assert!(try_eval("raid(5, 4 TB, 2)").is_err());
        assert!(try_eval("raid(10, 4 TB, 5)").is_err());
    }
}

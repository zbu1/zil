//! Hardware rules of thumb: wire gauges, RAID capacity and memory latency.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "hardware",
    about: "wire gauges, RAID capacity and memory timings",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Quick calculations for building and sizing hardware: how thick a wire is, how much space a RAID array leaves you, and how \
long memory takes to answer."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("hardware", &[
            ("how thick is 12 gauge wire?", "awg(12)"),
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

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn hardware() {
        assert_eq!(show("awg(12).round(3)"), "2.053 mm");
        assert_eq!(show("raid(5, 4 TB, 4)"), "12 TB");
        assert_eq!(show("raid(10, 2 TB, 6)"), "6 TB");
        assert_eq!(show("ram_latency(6000, 30)"), "10 ns");
        assert!(try_eval("raid(5, 4 TB, 2)").is_err());
        assert!(try_eval("raid(10, 4 TB, 5)").is_err());
    }
}

//! Every study of the catalogue, calculated over a hostile series and then updated after an
//! append and a change of a stored bar.

#![no_main]

use arbitrary::Arbitrary;
use chrono::Duration;
use egui_charts::studies::IndicatorFactory;
use libfuzzer_sys::fuzz_target;

#[path = "../common.rs"]
mod common;
use common::{FuzzBar, Prices};

#[derive(Arbitrary, Debug)]
struct Input {
    study: u16,
    bars: Vec<FuzzBar>,
    appended: Prices,
    changed: (u16, Prices),
}

fuzz_target!(|input: Input| {
    let factory = IndicatorFactory::new();
    let mut names = factory.list();
    names.sort();
    let name = &names[usize::from(input.study) % names.len()];
    let mut study = factory.create(name).expect("a listed study is registered");

    let mut bars: Vec<_> = input.bars.iter().map(FuzzBar::bar).collect();
    study.calculate(&bars);

    let last = bars.last().map(|b| b.time);
    if let Some(last) = last {
        let next = last.checked_add_signed(Duration::minutes(1));
        bars.push(input.appended.at(next.unwrap_or(last)));
        study.update(&bars, bars.len() - 1);
        let index = usize::from(input.changed.0) % bars.len();
        let time = bars[index].time;
        bars[index] = input.changed.1.at(time);
        study.update(&bars, index);
    }
    // Reading the result is part of the contract of a calculated study.
    let _ = (study.values().len(), study.line_count(), study.line_names());
});

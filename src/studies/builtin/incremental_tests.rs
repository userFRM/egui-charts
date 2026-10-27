use crate::studies::{
    CVD, CloseToCloseVol, EhlersFisher, HL2, HLC3, Indicator, IndicatorFactory, PriceChannel,
};

#[test]
fn bar_studies_support_tail_updates() {
    let factory = IndicatorFactory::new();
    let bars: Vec<_> = (0..3)
        .map(|i| {
            crate::model::Bar::new(
                chrono::DateTime::from_timestamp(i * 60, 0).unwrap(),
                100.0,
                101.0,
                99.0,
                100.0,
                1.0,
            )
        })
        .collect();
    let extra: [Box<dyn Indicator>; 6] = [
        Box::new(CloseToCloseVol::default()),
        Box::new(PriceChannel::default()),
        Box::new(HLC3::default()),
        Box::new(HL2::default()),
        Box::new(CVD::default()),
        Box::new(EhlersFisher::default()),
    ];
    let mut fallback: Vec<_> = factory
        .list()
        .into_iter()
        // Profiles have their own cached driver. A changed visible average moves its whole line.
        .filter(|name| {
            !matches!(
                name.as_str(),
                "Vol Profile Anchored"
                    | "Vol Profile Auto Anchored"
                    | "Vol Profile FR"
                    | "Vol Profile Periodic"
                    | "Vol Profile Session"
                    | "Vol Profile VR"
                    | "Visible Average Price"
            )
        })
        .filter(|name| {
            let mut study = factory.create(name).unwrap();
            study.calculate(&bars);
            !study.is_incremental()
        })
        .collect();
    fallback.extend(
        extra
            .iter()
            .filter(|study| !study.is_incremental())
            .map(|study| study.name().to_owned()),
    );
    assert!(
        fallback.is_empty(),
        "studies must update on every tick: {fallback:?}"
    );
}

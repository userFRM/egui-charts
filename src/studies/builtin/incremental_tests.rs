use crate::studies::IndicatorFactory;

#[test]
fn common_streaming_studies_support_tail_updates() {
    let factory = IndicatorFactory::new();
    let fallback: Vec<_> = [
        "KST",
        "Ichimoku",
        "PSAR",
        "WMA",
        "DEMA",
        "TEMA",
        "HMA",
        "Keltner",
        "Donchian",
        "ADX",
        "CCI",
        "Williams %R",
        "MFI",
        "ROC",
        "StochRSI",
        "TRIX",
        "Aroon",
        "CMF",
        "AO",
        "Envelopes",
        "StdDev",
        "VWMA",
    ]
    .into_iter()
    .filter(|name| !factory.create(name).unwrap().is_incremental())
    .collect();
    assert!(
        fallback.is_empty(),
        "studies must update on every tick: {fallback:?}"
    );
}

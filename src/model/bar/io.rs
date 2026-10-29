//! Bar Data I/O Operations
//!
//! CSV and JSON import/export for bar data. Errors are `Send + Sync` so imports
//! and exports can return their results across worker-thread boundaries.

use super::bar::Bar;
use super::bar_data::BarData;
use chrono::{DateTime, Utc};

/// The time column of a bar CSV file, UTC. `%.f` writes the fraction of a second only when the
/// time has one and reads a time with or without it.
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";

/// The header row of a bar CSV file, written by every writer and required by `from_csv`.
const HEADER: [&str; 6] = ["ts", "open", "high", "low", "close", "volume"];

impl BarData {
    /// Loads bars from a CSV file
    ///
    /// # Format
    /// CSV format: ts,open,high,low,close,volume. The header row is required (any case, spaces
    /// around names ignored) and the columns must be in this order.
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC), optionally with a fraction of a
    /// second, such as "2024-03-15 10:00:00.250", or a date alone, "2024-03-15", for midnight UTC
    /// of that day, the open of its day bar. A row of `NaN` values is a whitespace bar.
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::from_csv("data.csv").expect("Failed to load CSV");
    /// ```
    pub fn from_csv(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut rdr = csv::Reader::from_path(path)?;
        let header = rdr.headers()?;
        if !header
            .iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .eq(HEADER)
        {
            return Err(format!("Invalid CSV header: expected {}", HEADER.join(",")).into());
        }
        let mut bars = Vec::new();

        for (line_num, result) in rdr.records().enumerate() {
            let record =
                result.map_err(|e| format!("Error reading CSV line {}: {}", line_num + 2, e))?;

            if record.len() != 6 {
                return Err(format!(
                    "Invalid CSV format at line {}: expected 6 fields, got {}",
                    line_num + 2,
                    record.len()
                )
                .into());
            }

            let ts = record
                .get(0)
                .ok_or_else(|| format!("Missing ts at line {}", line_num + 2))?;
            let naive_dt = chrono::NaiveDateTime::parse_from_str(ts, TIME_FORMAT)
                .or_else(|e| {
                    chrono::NaiveDate::parse_from_str(ts, "%Y-%m-%d")
                        .map(|date| date.and_time(chrono::NaiveTime::MIN))
                        .map_err(|_| e)
                })
                .map_err(|e| format!("Invalid ts '{}' at line {}: {}", ts, line_num + 2, e))?;
            let ts = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);

            let open: f64 = record[1]
                .parse()
                .map_err(|e| format!("Invalid open price at line {}: {}", line_num + 2, e))?;
            let high: f64 = record[2]
                .parse()
                .map_err(|e| format!("Invalid high price at line {}: {}", line_num + 2, e))?;
            let low: f64 = record[3]
                .parse()
                .map_err(|e| format!("Invalid low price at line {}: {}", line_num + 2, e))?;
            let close: f64 = record[4]
                .parse()
                .map_err(|e| format!("Invalid close price at line {}: {}", line_num + 2, e))?;
            let volume: f64 = record[5]
                .parse()
                .map_err(|e| format!("Invalid volume at line {}: {}", line_num + 2, e))?;

            bars.push(Bar::new(ts, open, high, low, close, volume));
        }

        Ok(Self::from_bars(bars))
    }

    /// Exports bars to a CSV file
    ///
    /// # Format
    /// CSV format: ts,open,high,low,close,volume
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC), with the fraction of a second
    /// when the bar's time has one, such as "2024-03-15 10:00:00.250"
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// data.to_csv("output.csv").expect("Failed to export CSV");
    /// ```
    pub fn to_csv(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use std::fs::File;

        let file = File::create(path)?;
        let mut wtr = csv::Writer::from_writer(file);

        // Write header
        wtr.write_record(HEADER)?;

        // Write data
        for bar in &self.bars {
            wtr.write_record([
                bar.time.format(TIME_FORMAT).to_string(),
                bar.open.to_string(),
                bar.high.to_string(),
                bar.low.to_string(),
                bar.close.to_string(),
                bar.volume.to_string(),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

    /// Exports bars to a CSV string in memory
    ///
    /// # Format
    /// CSV format: ts,open,high,low,close,volume
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC), with the fraction of a second
    /// when the bar's time has one, such as "2024-03-15 10:00:00.250"
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// let csv = data.to_csv_string().expect("Failed to export CSV");
    /// assert!(csv.starts_with("ts,"));
    /// ```
    pub fn to_csv_string(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut wtr = csv::Writer::from_writer(Vec::new());

        // Write header
        wtr.write_record(HEADER)?;

        // Write data
        for bar in &self.bars {
            wtr.write_record([
                bar.time.format(TIME_FORMAT).to_string(),
                bar.open.to_string(),
                bar.high.to_string(),
                bar.low.to_string(),
                bar.close.to_string(),
                bar.volume.to_string(),
            ])?;
        }

        let bytes = wtr.into_inner()?;
        Ok(String::from_utf8(bytes)?)
    }

    /// Exports bars to a CSV string with indicator values alongside bar data
    ///
    /// # Format
    /// CSV format: ts,open,high,low,close,volume,indicator_1,indicator_2,...
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC), with the fraction of a second
    /// when the bar's time has one, such as "2024-03-15 10:00:00.250"
    ///
    /// Each indicator is a `(name, values)` pair where `values` is aligned to the
    /// bar data by index. Missing values (e.g. SMA warmup period) are empty strings.
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// let indicators: Vec<(String, Vec<f64>)> = vec![
    ///     ("SMA_20".to_string(), vec![]),
    /// ];
    /// let csv = data.to_csv_with_indicators(&indicators)
    ///     .expect("Failed to export CSV with indicators");
    /// assert!(csv.starts_with("ts,"));
    /// ```
    pub fn to_csv_with_indicators(
        &self,
        indicators: &[(String, Vec<f64>)],
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut wtr = csv::Writer::from_writer(Vec::new());

        // Build header: ts,open,high,low,close,volume,indicator_1,...
        let mut header: Vec<String> = HEADER.map(String::from).to_vec();
        for (name, _) in indicators {
            header.push(name.clone());
        }
        wtr.write_record(&header)?;

        // Write data rows
        for (i, bar) in self.bars.iter().enumerate() {
            let mut record: Vec<String> = vec![
                bar.time.format(TIME_FORMAT).to_string(),
                bar.open.to_string(),
                bar.high.to_string(),
                bar.low.to_string(),
                bar.close.to_string(),
                bar.volume.to_string(),
            ];

            for (_name, values) in indicators {
                if i < values.len() {
                    record.push(values[i].to_string());
                } else {
                    record.push(String::new());
                }
            }

            wtr.write_record(&record)?;
        }

        let bytes = wtr.into_inner()?;
        Ok(String::from_utf8(bytes)?)
    }

    /// Exports bars to a JSON file
    ///
    /// # Format
    /// JSON array of bar objects with full ISO 8601 timestamp
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// data.to_json("output.json").expect("Failed to export JSON");
    /// ```
    pub fn to_json(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use std::fs::File;
        use std::io::Write;

        let json = serde_json::to_string_pretty(&self.bars)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Returns JSON string representation of the bars
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// let json = data.to_json_string().expect("Failed to serialize");
    /// ```
    pub fn to_json_string(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::to_string_pretty(&self.bars)?)
    }

    /// Load bars from a JSON string
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let json = "[]";
    /// let data = std::thread::spawn(move || BarData::from_json_string(json))
    ///     .join().expect("Worker panicked").expect("Failed to parse");
    /// ```
    pub fn from_json_string(json: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bars: Vec<Bar> = serde_json::from_str(json)?;
        Ok(Self::from_bars(bars))
    }

    /// Load bars from a JSON file
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::from_json("data.json").expect("Failed to load");
    /// ```
    pub fn from_json(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use std::fs;
        let content = fs::read_to_string(path)?;
        Self::from_json_string(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_round_trip() {
        assert!(BarData::from_json_string("[]").unwrap().is_empty());
        let now = Utc::now();
        let mut bar = Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0);
        bar.colors = Some(Box::new(crate::model::BarColors {
            color: Some(egui::Color32::RED),
            ..Default::default()
        }));
        bar.payload = Some(std::sync::Arc::new(serde_json::json!({ "id": 7 })));
        let data = BarData::from_bars(vec![bar.clone(), Bar::whitespace(now)]);

        let json = data.to_json_string().unwrap();
        let parsed = BarData::from_json_string(&json).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.bars[0], bar);
        // JSON writes NaN as null, which reads back as NaN.
        assert!(parsed.bars[1].is_whitespace());

        // A date alone is midnight UTC, and a time alone is a whitespace bar.
        let parsed = BarData::from_json_string(
            r#"[{"time":"2024-03-15","open":1,"high":2,"low":0.5,"close":1.5,"volume":10},
                {"time":"2024-03-16"}]"#,
        )
        .unwrap();
        let midnight = |day| {
            format!("2024-03-{day}T00:00:00Z")
                .parse::<DateTime<Utc>>()
                .unwrap()
        };
        assert_eq!(
            parsed.bars[0],
            Bar::new(midnight(15), 1.0, 2.0, 0.5, 1.5, 10.0)
        );
        assert_eq!(parsed.bars[1].time, midnight(16));
        assert!(parsed.bars[1].is_whitespace());
    }

    #[test]
    fn test_csv_string_empty() {
        let data = BarData::new();
        let csv = data.to_csv_string().unwrap();
        let lines: Vec<&str> = csv.trim().lines().collect();

        // Header only
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "ts,open,high,low,close,volume");
    }

    #[test]
    fn test_csv_with_indicators() {
        let now = Utc::now();
        let data = BarData::from_bars(vec![
            Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0),
            Bar::new(now, 105.0, 115.0, 100.0, 112.0, 1500.0),
            Bar::new(now, 112.0, 120.0, 108.0, 118.0, 2000.0),
        ]);

        let indicators = vec![
            ("SMA_20".to_string(), vec![105.0, 107.5]),
            ("RSI_14".to_string(), vec![55.0, 60.0, 65.0]),
        ];

        let csv = data.to_csv_with_indicators(&indicators).unwrap();
        let lines: Vec<&str> = csv.trim().lines().collect();

        // Header + 3 data rows
        assert_eq!(lines.len(), 4);

        // Verify header includes indicator names
        assert!(lines[0].contains("SMA_20"));
        assert!(lines[0].contains("RSI_14"));

        // Third row: SMA has no value (index 2, but SMA only has 2 values)
        // so that cell should be empty
        let row3_fields: Vec<&str> = lines[3].split(',').collect();
        assert_eq!(row3_fields.len(), 8); // 6 bar fields + 2 indicators
        assert_eq!(row3_fields[6], ""); // SMA missing for bar index 2
        assert_eq!(row3_fields[7], "65"); // RSI present
    }

    #[test]
    fn test_csv_with_no_indicators() {
        let now = Utc::now();
        let data = BarData::from_bars(vec![Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0)]);

        let indicators: Vec<(String, Vec<f64>)> = vec![];
        let csv = data.to_csv_with_indicators(&indicators).unwrap();
        let csv_plain = data.to_csv_string().unwrap();

        // With empty indicators, should be identical to plain CSV
        assert_eq!(csv, csv_plain);
    }

    /// A path in the temporary directory, unique to this process and `name`.
    fn temp_csv(name: &str) -> String {
        let file = format!("egui-charts-{}-{name}.csv", std::process::id());
        std::env::temp_dir().join(file).to_str().unwrap().to_owned()
    }

    #[test]
    fn bar_csv_round_trips_sub_second_times() {
        let start = DateTime::parse_from_rfc3339("2024-03-15T10:00:00Z")
            .unwrap()
            .to_utc();
        let times: Vec<DateTime<Utc>> = (0..3)
            .map(|i| start + chrono::Duration::milliseconds(100 * i))
            .collect();
        let bars = times
            .iter()
            .map(|&t| Bar::new(t, 1.0, 2.0, 0.5, 1.5, 10.0))
            .collect();
        let path = temp_csv("sub-second");

        BarData::from_bars(bars).to_csv(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let loaded = BarData::from_csv(&path);
        std::fs::remove_file(&path).unwrap();

        let written_times: Vec<&str> = written
            .lines()
            .skip(1)
            .map(|row| row.split(',').next().unwrap())
            .collect();
        assert_eq!(
            written_times,
            [
                "2024-03-15 10:00:00",
                "2024-03-15 10:00:00.100",
                "2024-03-15 10:00:00.200"
            ]
        );
        let loaded_times: Vec<DateTime<Utc>> =
            loaded.unwrap().bars.iter().map(|b| b.time).collect();
        assert_eq!(loaded_times, times);
    }

    #[test]
    fn from_csv_checks_the_header_and_reads_date_only_times() {
        let refused = Err("Invalid CSV header: expected ts,open,high,low,close,volume".to_string());
        let cases = [
            (
                "2024-03-15 10:00:00,1,2,0.5,1.5,10\n2024-03-15 10:01:00,1,2,0.5,1.5,10\n",
                refused.clone(),
            ),
            (
                "ts,close,high,low,open,volume\n2024-03-15 10:00:00,1.5,2,0.5,1,10\n",
                refused,
            ),
            (
                " TS , Open,HIGH,low,close,Volume\n2024-03-15 10:00:00,1,2,0.5,1.5,10\n",
                Ok(vec![("2024-03-15 10:00:00 UTC".to_string(), 1.0, 1.5)]),
            ),
            // A date alone is midnight UTC.
            (
                "ts,open,high,low,close,volume\n2024-03-15,1,2,0.5,1.5,10\n",
                Ok(vec![("2024-03-15 00:00:00 UTC".to_string(), 1.0, 1.5)]),
            ),
        ];
        let path = temp_csv("header");
        for (contents, expected) in cases {
            std::fs::write(&path, contents).unwrap();
            let got = BarData::from_csv(&path)
                .map(|data| {
                    data.bars
                        .iter()
                        .map(|b| (b.time.to_string(), b.open, b.close))
                        .collect::<Vec<_>>()
                })
                .map_err(|e| e.to_string());
            assert_eq!(got, expected, "{contents:?}");
        }
        std::fs::remove_file(&path).unwrap();
    }
}

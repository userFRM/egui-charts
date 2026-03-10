//! Bar Data I/O Operations
//!
//! CSV and JSON import/export for bar data.

use super::bar::Bar;
use super::bar_data::BarData;
use chrono::{DateTime, Utc};

impl BarData {
    /// Loads bars from a CSV file
    ///
    /// # Format
    /// CSV format: ts,open,high,low,close,volume
    /// Timestamps should be in "YYYY-MM-DD HH:MM:SS" format (UTC)
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::from_csv("data.csv").expect("Failed to load CSV");
    /// ```
    pub fn from_csv(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut rdr = csv::Reader::from_path(path)?;
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
            let naive_dt = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
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
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC)
    ///
    /// # Example
    /// ```no_run
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// data.to_csv("output.csv").expect("Failed to export CSV");
    /// ```
    pub fn to_csv(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;

        let file = File::create(path)?;
        let mut wtr = csv::Writer::from_writer(file);

        // Write header
        wtr.write_record(["ts", "open", "high", "low", "close", "volume"])?;

        // Write data
        for bar in &self.bars {
            wtr.write_record([
                bar.time.format("%Y-%m-%d %H:%M:%S").to_string(),
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
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC)
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let data = BarData::new();
    /// let csv = data.to_csv_string().expect("Failed to export CSV");
    /// assert!(csv.starts_with("ts,"));
    /// ```
    pub fn to_csv_string(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut wtr = csv::Writer::from_writer(Vec::new());

        // Write header
        wtr.write_record(["ts", "open", "high", "low", "close", "volume"])?;

        // Write data
        for bar in &self.bars {
            wtr.write_record([
                bar.time.format("%Y-%m-%d %H:%M:%S").to_string(),
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
    /// Timestamps are in "YYYY-MM-DD HH:MM:SS" format (UTC)
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
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut wtr = csv::Writer::from_writer(Vec::new());

        // Build header: ts,open,high,low,close,volume,indicator_1,...
        let mut header: Vec<String> = vec![
            "ts".to_string(),
            "open".to_string(),
            "high".to_string(),
            "low".to_string(),
            "close".to_string(),
            "volume".to_string(),
        ];
        for (name, _) in indicators {
            header.push(name.clone());
        }
        wtr.write_record(&header)?;

        // Write data rows
        for (i, bar) in self.bars.iter().enumerate() {
            let mut record: Vec<String> = vec![
                bar.time.format("%Y-%m-%d %H:%M:%S").to_string(),
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
    pub fn to_json(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    pub fn to_json_string(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(serde_json::to_string_pretty(&self.bars)?)
    }

    /// Load bars from a JSON string
    ///
    /// # Example
    /// ```
    /// use egui_charts::model::BarData;
    ///
    /// let json = "[]";
    /// let data = BarData::from_json_string(json).expect("Failed to parse");
    /// ```
    pub fn from_json_string(json: &str) -> Result<Self, Box<dyn std::error::Error>> {
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
    pub fn from_json(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
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
        let now = Utc::now();
        let data = BarData::from_bars(vec![Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0)]);

        let json = data.to_json_string().unwrap();
        let parsed = BarData::from_json_string(&json).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.bars[0].open, 100.0);
    }

    #[test]
    fn test_empty_json() {
        let data = BarData::from_json_string("[]").unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_csv_string_basic() {
        let now = Utc::now();
        let data = BarData::from_bars(vec![Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0)]);

        let csv = data.to_csv_string().unwrap();
        let lines: Vec<&str> = csv.trim().lines().collect();

        // Header + 1 data row
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("ts,"));
        assert!(lines[0].contains("open"));
        assert!(lines[0].contains("volume"));
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
}

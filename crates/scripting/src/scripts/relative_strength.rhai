// Relative strength from Wilder's average gain and loss.
display(overlay = false, format = 2);

input length = length("Length", 14, 1, 500);

state previous = na;    // the last finite close
state count = 0;        // changes seen
state up = 0.0;         // average gain
state down = 0.0;       // average loss

let value = na;
if !is_na(close) {
    if !is_na(previous) {
        let change = close - previous;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        count += 1;
        if count <= length {
            up += gain;
            down += loss;
            if count == length {
                up /= length;
                down /= length;
            }
        } else {
            up = (up * (length - 1) + gain) / length;
            down = (down * (length - 1) + loss) / length;
        }
        if count >= length {
            value = if up + down > 0.0 { 100.0 * up / (up + down) } else { 50.0 };
        }
    }
    previous = close;
}
plot("RSI", value, color = "#7E57C2");
plot("Upper", 70.0);
plot("Lower", 30.0);
fill("Upper", "Lower", color = "#7E57C220");

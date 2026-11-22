// The highest high and lowest low of the bars before, marked where the close leaves them.
display(overlay = true);
input length = length("Length", 20, 1, 500);
series upper = highest(high, length);
series lower = lowest(low, length);
plot("Upper", upper[1]);
plot("Lower", lower[1]);
plot("Breakout", if close > upper[1] { close } else { na }, style = "triangle_up");
plot("Breakdown", if close < lower[1] { close } else { na }, style = "triangle_down");

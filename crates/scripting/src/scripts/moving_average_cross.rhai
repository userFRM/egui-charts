// Two exponential averages of the close, marked where they cross.
display(overlay = true);
input fast = length("Fast", 9, 1, 500);
input slow = length("Slow", 21, 1, 500);
let f = ema(close, fast);
let s = ema(close, slow);
plot("Fast", f, color = "#2962FF");
plot("Slow", s, color = "#FF6D00");
plot("Cross", if crossover(f, s) || crossunder(f, s) { close } else { na }, style = "circles");

/* An illustration, not a simulation or sensor feed. SVG stays visible without canvas. */
(() => {
  "use strict";
  const canvas = document.getElementById("airflow");
  const context = canvas.getContext("2d");
  if (!context) return;
  const button = document.getElementById("motion-toggle");
  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
  let paused = reducedMotion.matches;
  let visible = true;
  let frame = 0;
  let previousTime = 0;
  let phase = 0;
  let size = 0;

  // Project a gently tilted sphere. Rear currents are lighter than front ones.
  function project(longitude, latitude) {
    const radius = 203 * Math.cos(latitude);
    const x = radius * Math.cos(longitude);
    const y = 203 * Math.sin(latitude);
    const depth = radius * Math.sin(longitude);
    const tilt = -0.55;
    return {
      x: 300 + x * Math.cos(tilt) - y * Math.sin(tilt),
      y: 300 + x * Math.sin(tilt) + y * Math.cos(tilt),
      depth,
    };
  }

  function drawCurrent(latitude, offset) {
    context.beginPath();
    for (let step = 0; step <= 80; step++) {
      const longitude = (step / 80) * Math.PI * 2;
      const ripple = Math.sin(longitude * 3 + phase + offset) * 0.026;
      const point = project(longitude, latitude + ripple);
      if (step === 0) context.moveTo(point.x, point.y);
      else context.lineTo(point.x, point.y);
    }
    context.strokeStyle = "rgba(53, 130, 105, 0.16)";
    context.lineWidth = 0.7;
    context.stroke();
  }

  function drawParticle(index) {
    const latitude = ((index % 13) / 12 - 0.5) * 2.6;
    const longitude = index * 2.39996 + phase * (0.16 + (index % 3) * 0.04);
    const point = project(longitude, latitude);
    const opacity = 0.2 + ((point.depth + 203) / 406) * 0.5;
    context.beginPath();
    context.arc(point.x, point.y, index % 5 === 0 ? 2.2 : 1.1, 0, Math.PI * 2);
    context.fillStyle = `rgba(34, 112, 87, ${opacity})`;
    context.fill();
  }

  function draw() {
    context.clearRect(0, 0, 600, 600);
    for (let band = 0; band < 23; band++) {
      drawCurrent((band / 22 - 0.5) * 2.9, band * 0.3);
    }
    for (let particle = 0; particle < 65; particle++) drawParticle(particle);
  }

  function tick(time) {
    if (!previousTime) previousTime = time;
    const elapsed = time - previousTime;
    if (elapsed >= 1000 / 30) {
      phase += Math.min(elapsed, 100) / 1000;
      draw();
      previousTime = time;
    }
    frame = requestAnimationFrame(tick);
  }

  function synchronize() {
    cancelAnimationFrame(frame);
    frame = 0;
    previousTime = 0;
    button.textContent = paused ? "Play motion" : "Pause motion";
    canvas.dataset.motion = paused || !visible || document.hidden ? "still" : "running";
    if (canvas.dataset.motion === "running") frame = requestAnimationFrame(tick);
  }

  function resize() {
    const nextSize = canvas.getBoundingClientRect().width;
    if (!nextSize || nextSize === size) return;
    size = nextSize;
    const pixels = Math.round(size * Math.min(devicePixelRatio || 1, 2));
    canvas.width = pixels;
    canvas.height = pixels;
    context.setTransform(pixels / 600, 0, 0, pixels / 600, 0, 0);
    draw();
  }

  button.hidden = false;
  button.addEventListener("click", () => { paused = !paused; synchronize(); });
  reducedMotion.addEventListener("change", () => {
    if (reducedMotion.matches) paused = true;
    synchronize();
  });
  document.addEventListener("visibilitychange", synchronize);
  new ResizeObserver(resize).observe(canvas);
  new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    synchronize();
  }).observe(canvas);
  resize();
  synchronize();
})();

const canvas = document.querySelector("canvas");
const context = canvas.getContext("2d");
const decoder = new TextDecoder();
let wasm;

const color = (red, green, blue, alpha) =>
  `rgba(${red}, ${green}, ${blue}, ${alpha / 255})`;
const imports = {
  canvas: {
    clear() {
      context.clearRect(0, 0, canvas.clientWidth, canvas.clientHeight);
    },
    fill_rect(x, y, width, height, radius, red, green, blue, alpha) {
      context.fillStyle = color(red, green, blue, alpha);
      context.beginPath();
      if (radius > 0) {
        context.roundRect(x, y, width, height, Math.min(radius, width / 2, height / 2));
      } else {
        context.rect(x, y, width, height);
      }
      context.fill();
    },
    fill_text(pointer, length, x, y, size, red, green, blue, alpha) {
      const text = decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length));
      context.font = `${size}px ui-monospace, monospace`;
      context.textBaseline = "top";
      context.fillStyle = color(red, green, blue, alpha);
      context.fillText(text, x, y);
    },
    measure_text(pointer, length, size) {
      const text = decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length));
      context.font = `${size}px ui-monospace, monospace`;
      return context.measureText(text).width;
    },
    report_error(pointer, length) {
      console.error(decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length)));
    },
  },
};

function draw(event = 0, x = 0, y = 0) {
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  const scale = window.devicePixelRatio || 1;
  const pixelWidth = Math.round(width * scale);
  const pixelHeight = Math.round(height * scale);
  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
  }
  context.setTransform(scale, 0, 0, scale, 0, 0);
  wasm.render(width, height, performance.now(), event, x, y);
}

function pointer(event, kind) {
  const bounds = canvas.getBoundingClientRect();
  draw(kind, event.clientX - bounds.left, event.clientY - bounds.top);
}

try {
  const response = await fetch("../target/wasm32-unknown-unknown/debug/blit_web.wasm");
  if (!response.ok) throw new Error(`WASM fetch failed: ${response.status}`);
  wasm = (await WebAssembly.instantiate(await response.arrayBuffer(), imports)).instance.exports;
  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    canvas.setPointerCapture(event.pointerId);
    pointer(event, 1);
  });
  canvas.addEventListener("pointerup", (event) => pointer(event, 2));
  canvas.addEventListener("pointermove", (event) => pointer(event, 3));
  canvas.addEventListener("pointerleave", () => draw(4));
  window.addEventListener("resize", () => draw());
  draw();
} catch (error) {
  const message = document.querySelector("#error");
  message.hidden = false;
  message.textContent = `Couldn't start Blit: ${error.message}\n\nBuild it from the repo root:\nCARGO_ENCODED_RUSTFLAGS= cargo -Z build-std=std,panic_abort build -p blit-web --target wasm32-unknown-unknown`;
  console.error(error);
}

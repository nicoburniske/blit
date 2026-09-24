export async function mount(canvas, wasmUrl) {
  const document = canvas.ownerDocument;
  const window = document.defaultView;
  const context = canvas.getContext("2d");
  const decoder = new TextDecoder();
  const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  const layouts = new Map();
  const clips = [];
  const actions = [];
  const ranges = [];
  const texts = [];
  const semanticBounds = new WeakMap();
  let actionCount = 0;
  let rangeCount = 0;
  let textCount = 0;
  let semanticPrevious = null;
  let documentHeight = 0;
  let pendingFrame = 0;
  let activePointer = null;
  let spaceButton = null;
  let mousePosition = null;
  let exports;
  let failure;

  const overlay = document.createElement("div");
  overlay.dataset.blitOverlay = "";
  const spacer = document.createElement("div");
  spacer.dataset.blitSpacer = "";
  spacer.setAttribute("aria-hidden", "true");
  const errorMessage = document.createElement("div");
  errorMessage.dataset.blitError = "";
  errorMessage.setAttribute("role", "alert");
  errorMessage.hidden = true;
  const notice = document.createElement("div");
  notice.dataset.blitNotice = "";
  notice.setAttribute("role", "status");
  notice.setAttribute("aria-live", "polite");
  notice.setAttribute("aria-atomic", "true");
  const style = document.createElement("style");
  style.textContent = `
    [data-blit-overlay] {
      position: absolute; top: 0; left: 0; z-index: 1;
      pointer-events: none; overflow: clip;
    }
    [data-blit-overlay] > * {
      position: absolute; display: block; box-sizing: border-box;
      margin: 0; padding: 0; border: 0; background: transparent;
      color: transparent; text-shadow: none; font-size: 1px;
      line-height: 1; overflow: hidden; white-space: pre-wrap;
    }
    [data-blit-overlay] > button, [data-blit-overlay] > a {
      pointer-events: auto; cursor: pointer; touch-action: auto;
      text-decoration: none; appearance: none;
    }
    [data-blit-overlay] > input[type="range"] {
      pointer-events: auto; cursor: pointer; appearance: none;
      min-width: 0; direction: ltr; touch-action: pan-y pinch-zoom;
    }
    [data-blit-overlay] > input[type="range"]:disabled { cursor: default; }
    [data-blit-overlay] > input[type="range"]::-webkit-slider-runnable-track {
      height: 16px; border: 0; background: transparent; opacity: 0;
    }
    [data-blit-overlay] > input[type="range"]::-webkit-slider-thumb {
      appearance: none; width: 16px; height: 16px; margin: 0;
      border: 0; border-radius: 0; background: transparent;
    }
    [data-blit-overlay] > input[type="range"]::-moz-range-track,
    [data-blit-overlay] > input[type="range"]::-moz-range-progress {
      height: 16px; border: 0; background: transparent; opacity: 0;
    }
    [data-blit-overlay] > input[type="range"]::-moz-range-thumb {
      width: 16px; height: 16px; border: 0; border-radius: 0;
      background: transparent; opacity: 0;
    }
    [data-blit-overlay] > :focus-visible {
      outline: 2px solid Highlight; outline-offset: -2px;
    }
    [data-blit-spacer] { width: 1px; pointer-events: none; }
    [data-blit-error]:not([hidden]) {
      position: fixed; inset: 16px 16px auto; z-index: 2;
      padding: 16px; background: white; color: #900;
      font: 16px/1.5 system-ui, sans-serif; white-space: pre-wrap;
    }
    [data-blit-notice] {
      position: fixed; bottom: 16px; left: 16px; z-index: 2;
      max-width: calc(100% - 32px); box-sizing: border-box;
      pointer-events: none; font: 14px/1.5 system-ui, sans-serif;
    }
    [data-blit-notice]:not(:empty) {
      padding: 12px; background: white; color: #222;
    }
  `;
  document.head.append(style);
  document.body.style.margin = "0";
  document.documentElement.style.scrollbarGutter = "stable";
  Object.assign(canvas.style, {
    position: "fixed",
    top: "0",
    left: "0",
    display: "block",
    touchAction: "auto",
  });
  canvas.setAttribute("aria-hidden", "true");
  canvas.parentElement.append(overlay);
  document.body.append(spacer, errorMessage, notice);

  function draw(event = 0, x = 0, y = 0) {
    if (!exports || failure) return;
    window.cancelAnimationFrame(pendingFrame);
    pendingFrame = 0;
    try {
      const focused = overlay.contains(document.activeElement) ? document.activeElement : null;
      const width = document.documentElement.clientWidth;
      const height = window.innerHeight;
      const scale = window.devicePixelRatio || 1;
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      spacer.style.height = `${Math.max(height, Math.ceil(documentHeight))}px`;
      overlay.style.width = `${width}px`;
      overlay.style.height = spacer.style.height;
      const pixelWidth = Math.round(width * scale);
      const pixelHeight = Math.round(height * scale);
      if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
        canvas.width = pixelWidth;
        canvas.height = pixelHeight;
      }
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.translate(0, -window.scrollY);
      layouts.clear();
      clips.length = 0;
      actionCount = 0;
      rangeCount = 0;
      textCount = 0;
      semanticPrevious = null;
      const redraw = exports.render(
        width,
        Math.max(height, documentHeight),
        window.performance.now(),
        event,
        x,
        y,
      );
      while (actions.length > actionCount) actions.pop().remove();
      while (ranges.length > rangeCount) ranges.pop().remove();
      while (texts.length > textCount) texts.pop().remove();
      for (const element of texts) {
        const bounds = semanticBounds.get(element);
        const duplicate = element.localName === "span" && !element.hasAttribute("aria-live")
          && actions.some(action => {
            if (action.hidden || action.textContent !== element.textContent) return false;
            const area = semanticBounds.get(action);
            return bounds.left >= area.left - 0.01 && bounds.top >= area.top - 0.01
              && bounds.right <= area.right + 0.01 && bounds.bottom <= area.bottom + 0.01;
          });
        if (duplicate) element.setAttribute("aria-hidden", "true");
        else element.removeAttribute("aria-hidden");
      }
      if (focused?.isConnected && document.activeElement !== focused) {
        focused.focus({ preventScroll: true });
      }
      if (redraw) schedule();
    } catch (error) {
      reportError(error);
    }
  }

  function schedule() {
    if (pendingFrame || failure) return;
    pendingFrame = window.requestAnimationFrame(() => {
      pendingFrame = 0;
      if (mousePosition) {
        draw(3, mousePosition.x, mousePosition.y + window.scrollY);
      } else {
        draw();
      }
    });
  }

  function reportError(error) {
    if (failure) return;
    failure = error instanceof Error ? error : new Error(String(error));
    errorMessage.textContent = `Blit: ${failure.message}`;
    errorMessage.hidden = false;
    window.cancelAnimationFrame(pendingFrame);
    console.error(failure);
    canvas.dispatchEvent(new CustomEvent("blit-error", { detail: failure }));
  }

  function decode(pointer, length) {
    return decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length));
  }

  function layoutText(text, width, size, font, weight, lineHeight) {
    const family = ["ui-monospace, monospace", "ui-serif, serif", "system-ui, sans-serif"][font];
    context.font = `${weight} ${size}px ${family}`;
    const key = JSON.stringify([text, width, size, font, weight, lineHeight]);
    if (layouts.has(key)) return layouts.get(key);
    const lines = [];
    const spaceWidth = context.measureText(" ").width;
    for (const paragraph of text.split(/\r\n|\r|\n/u)) {
      let line = "";
      for (const [word] of paragraph.matchAll(/[ \t]+|[^ \t]+/gu)) {
        if (/^[ \t]+$/u.test(word)) {
          for (const character of word) {
            if (character === "\t" && spaceWidth > 0) {
              const column = context.measureText(line).width / spaceWidth;
              line += " ".repeat(Math.max(1, Math.round(8 - column % 8)));
            } else {
              line += character;
            }
          }
          continue;
        }
        const candidate = line + word;
        if (Math.fround(context.measureText(candidate).width) <= width) {
          line = candidate;
          continue;
        }
        if (line) lines.push(line);
        line = "";
        if (Math.fround(context.measureText(word).width) <= width) {
          line = word;
          continue;
        }
        for (const { segment } of graphemes.segment(word)) {
          const candidate = line + segment;
          if (line && Math.fround(context.measureText(candidate).width) > width) {
            lines.push(line);
            line = segment;
          } else {
            line = candidate;
          }
        }
      }
      lines.push(line);
    }
    const metrics = context.measureText("Mg");
    const ascent = metrics.fontBoundingBoxAscent ?? metrics.actualBoundingBoxAscent;
    const descent = metrics.fontBoundingBoxDescent ?? metrics.actualBoundingBoxDescent;
    const leading = size * lineHeight;
    let measuredWidth = 0;
    for (const line of lines) {
      measuredWidth = Math.max(measuredWidth, Math.fround(context.measureText(line).width));
    }
    const layout = {
      lines,
      width: Math.min(width, measuredWidth),
      height: Math.fround(lines.length * leading),
      leading,
      baseline: (leading - ascent - descent) / 2 + ascent,
    };
    layouts.set(key, layout);
    return layout;
  }

  function semantic(tag, text, href, left, top, width, height, live = false, selected = 0) {
    const action = tag === "button" || tag === "a";
    const range = tag === "input";
    const pool = range ? ranges : action ? actions : texts;
    const index = range ? rangeCount++ : action ? actionCount++ : textCount++;
    let element = pool[index];
    if (!element || element.localName !== tag) {
      element?.remove();
      element = document.createElement(tag);
      if (tag === "button") element.type = "button";
      if (range) element.type = "range";
      pool[index] = element;
    }
    if (action) {
      if (tag === "button" && selected !== 0) {
        element.setAttribute("aria-pressed", selected === 2 ? "true" : "false");
      } else {
        element.removeAttribute("aria-pressed");
      }
      if (tag === "a" && selected === 2) element.setAttribute("aria-current", "page");
      else element.removeAttribute("aria-current");
    } else {
      if (live && tag === "span") element.setAttribute("role", "status");
      else element.removeAttribute("role");
      if (live) {
        element.setAttribute("aria-live", "polite");
        element.setAttribute("aria-atomic", "true");
      } else {
        element.removeAttribute("aria-live");
        element.removeAttribute("aria-atomic");
      }
    }
    if (element.textContent !== text) element.textContent = text;
    if (tag === "a" && element.getAttribute("href") !== href) {
      element.setAttribute("href", href);
    }
    const clip = clips.at(-1);
    let visible = width > 0 && height > 0;
    if (clip && range) {
      element.style.clipPath = `inset(${Math.max(0, clip.top - top)}px ${Math.max(0, left + width - clip.right)}px ${Math.max(0, top + height - clip.bottom)}px ${Math.max(0, clip.left - left)}px)`;
      visible &&= left < clip.right && top < clip.bottom
        && left + width > clip.left && top + height > clip.top;
    } else if (clip) {
      const right = Math.min(left + width, clip.right);
      const bottom = Math.min(top + height, clip.bottom);
      left = Math.max(left, clip.left);
      top = Math.max(top, clip.top);
      width = Math.max(0, right - left);
      height = Math.max(0, bottom - top);
    } else if (range) {
      element.style.clipPath = "";
    }
    Object.assign(element.style, {
      left: `${left}px`,
      top: `${top}px`,
      width: `${width}px`,
      height: `${height}px`,
    });
    semanticBounds.set(element, { left, top, right: left + width, bottom: top + height });
    element.hidden = !visible || width <= 0 || height <= 0;
    if (element.hidden) element.style.display = "none";
    else element.style.removeProperty("display");
    const next = semanticPrevious ? semanticPrevious.nextSibling : overlay.firstChild;
    if (next !== element) {
      if (overlay.moveBefore && element.parentElement === overlay) overlay.moveBefore(element, next);
      else overlay.insertBefore(element, next);
    }
    semanticPrevious = element;
    return element;
  }

  const imports = {
    canvas: {
      clear() {
        context.save();
        context.resetTransform();
        context.clearRect(0, 0, canvas.width, canvas.height);
        context.restore();
      },
      fill_rect(left, top, width, height, radius, color, border, borderWidth) {
        if (width <= 0 || height <= 0) return;
        radius = Math.max(0, Math.min(radius, width / 2, height / 2));
        context.beginPath();
        context.roundRect(left, top, width, height, radius);
        context.fillStyle = `#${(color >>> 0).toString(16).padStart(8, "0")}`;
        context.fill();
        borderWidth = Math.max(0, Math.min(borderWidth, width / 2, height / 2));
        if (borderWidth > 0) {
          const inset = borderWidth / 2;
          context.beginPath();
          context.roundRect(
            left + inset, top + inset, width - borderWidth, height - borderWidth,
            Math.max(0, radius - inset),
          );
          context.lineWidth = borderWidth;
          context.strokeStyle = `#${(border >>> 0).toString(16).padStart(8, "0")}`;
          context.stroke();
        }
      },
      measure_text(pointer, length, width, size, font, weight, lineHeight, measured) {
        const layout = layoutText(decode(pointer, length), width, size, font, weight, lineHeight);
        const memory = new DataView(exports.memory.buffer);
        memory.setFloat32(measured, layout.width, true);
        memory.setFloat32(measured + 4, layout.height, true);
      },
      fill_text(pointer, length, left, top, width, height, size, font, weight, lineHeight, color, heading = 0, live = 0) {
        const text = decode(pointer, length);
        const layout = layoutText(text, width, size, font, weight, lineHeight);
        context.save();
        context.beginPath();
        context.rect(left, top, width, height);
        context.clip();
        context.textAlign = "left";
        context.textBaseline = "alphabetic";
        context.fillStyle = `#${(color >>> 0).toString(16).padStart(8, "0")}`;
        for (let index = 0; index < layout.lines.length; index++) {
          context.fillText(layout.lines[index], left, top + layout.baseline + index * layout.leading);
        }
        context.restore();
        semantic(heading ? `h${heading}` : "span", text, null, left, top, width, height, live !== 0);
      },
      action(pointer, length, href, hrefLength, left, top, width, height, selected = 0) {
        semantic(
          href ? "a" : "button", decode(pointer, length), href ? decode(href, hrefLength) : null,
          left, top, width, height, false, selected,
        );
      },
      range(label, labelLength, valueText, valueTextLength, minimum, maximum, value, left, top, width, height) {
        minimum >>>= 0;
        maximum >>>= 0;
        value >>>= 0;
        const element = semantic("input", "", null, left, top, width, height);
        if (element.min !== String(minimum)) element.min = String(minimum);
        if (element.max !== String(maximum)) element.max = String(maximum);
        element.step = "1";
        element.disabled = minimum === maximum;
        if (element.valueAsNumber !== value) element.value = String(value);
        element.setAttribute("aria-label", decode(label, labelLength));
        if (valueTextLength) element.setAttribute("aria-valuetext", decode(valueText, valueTextLength));
        else element.removeAttribute("aria-valuetext");
      },
      push_clip(left, top, width, height) {
        context.save();
        context.beginPath();
        context.rect(left, top, width, height);
        context.clip();
        const previous = clips.at(-1);
        clips.push({
          left: Math.max(left, previous?.left ?? -Infinity),
          top: Math.max(top, previous?.top ?? -Infinity),
          right: Math.min(left + width, previous?.right ?? Infinity),
          bottom: Math.min(top + height, previous?.bottom ?? Infinity),
        });
      },
      pop_clip() {
        context.restore();
        clips.pop();
      },
    },
    browser: {
      set_document_height(height) {
        if (!Number.isFinite(height) || height < 0) {
          throw new RangeError("document height must be finite and nonnegative");
        }
        if (documentHeight === height) return;
        documentHeight = height;
        spacer.style.height = `${Math.max(window.innerHeight, Math.ceil(height))}px`;
        overlay.style.height = spacer.style.height;
        schedule();
      },
      navigate(pointer, length) {
        window.location.assign(decode(pointer, length));
      },
      async copy_text(pointer, length) {
        const text = decode(pointer, length);
        notice.textContent = "";
        try {
          await window.navigator.clipboard.writeText(text);
          notice.textContent = "Copied to clipboard.";
        } catch {
          notice.textContent = "Could not copy: clipboard access is unavailable.";
        }
      },
      set_cursor(pointer) {
        canvas.style.cursor = pointer ? "pointer" : "default";
      },
      report_error(pointer, length) {
        reportError(decode(pointer, length));
      },
    },
  };

  try {
    if (!context) throw new Error("Canvas 2D is unavailable");
    const response = await window.fetch(wasmUrl);
    if (!response.ok) throw new Error(`WASM fetch failed: ${response.status}`);
    const result = await WebAssembly.instantiate(await response.arrayBuffer(), imports);
    exports = result.instance.exports;
    if (!(exports.memory instanceof WebAssembly.Memory) || typeof exports.render !== "function") {
      throw new Error("WASM must export memory and render");
    }
    draw();
    if (failure) throw failure;
  } catch (error) {
    reportError(error);
    throw error;
  }

  function pointer(event, kind) {
    const bounds = canvas.getBoundingClientRect();
    const left = event.clientX - bounds.left;
    const top = event.clientY - bounds.top;
    mousePosition = event.pointerType === "mouse" ? { x: left, y: top } : null;
    draw(kind, left, top + window.scrollY);
  }

  function cancelPointer() {
    spaceButton = null;
    if (activePointer !== null) {
      activePointer = null;
      draw(2, -1e9, -1e9);
    }
    mousePosition = null;
    draw(4);
  }

  document.addEventListener("pointerdown", (event) => {
    if (!event.isPrimary || event.button !== 0 || activePointer !== null) return;
    if (event.target !== canvas && !overlay.contains(event.target)) return;
    if (event.target.closest('a, input[type="range"]')) return;
    activePointer = event.pointerId;
    pointer(event, 1);
  }, { passive: true });
  document.addEventListener("pointermove", (event) => {
    if (event.target.matches('input[type="range"]')) return;
    if (!event.isPrimary || (activePointer !== null && activePointer !== event.pointerId)) return;
    if (activePointer === null && event.target !== canvas && !overlay.contains(event.target)) return;
    pointer(event, 3);
  }, { passive: true });
  document.addEventListener("pointerup", (event) => {
    if (activePointer !== event.pointerId) return;
    activePointer = null;
    pointer(event, 2);
    if (event.pointerType !== "mouse") cancelPointer();
  }, { passive: true });
  document.addEventListener("pointercancel", (event) => {
    if (activePointer === event.pointerId) cancelPointer();
  }, { passive: true });
  document.addEventListener("lostpointercapture", (event) => {
    if (activePointer === event.pointerId) cancelPointer();
  }, { passive: true });
  document.addEventListener("pointerout", (event) => {
    if (event.relatedTarget === null) {
      mousePosition = null;
      draw(4);
    }
  }, { passive: true });
  window.addEventListener("blur", cancelPointer);
  overlay.addEventListener("input", (event) => {
    const range = event.target;
    if (!range.matches('input[type="range"]') || range.disabled) return;
    const minimum = Number(range.min);
    const maximum = Number(range.max);
    if (maximum <= minimum) return;
    const fraction = (range.valueAsNumber - minimum) / (maximum - minimum);
    const bounds = range.getBoundingClientRect();
    const left = bounds.left + 8 + fraction * Math.max(0, bounds.width - 16);
    const top = bounds.top + bounds.height / 2 + window.scrollY;
    mousePosition = null;
    draw(1, left, top);
    draw(2, left, top);
    draw(4);
  });
  overlay.addEventListener("keydown", (event) => {
    const button = event.target.closest("button");
    if (!button || (event.key !== "Enter" && event.key !== " ")) return;
    event.preventDefault();
    if (event.repeat) return;
    if (event.key === "Enter") button.click();
    else spaceButton = button;
  });
  overlay.addEventListener("keyup", (event) => {
    if (event.key !== " " || !spaceButton) return;
    event.preventDefault();
    const button = spaceButton;
    spaceButton = null;
    if (button === document.activeElement) button.click();
  });
  overlay.addEventListener("focusout", () => { spaceButton = null; });
  overlay.addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (!button || event.detail !== 0) return;
    const bounds = button.getBoundingClientRect();
    const left = bounds.left + bounds.width / 2;
    const top = bounds.top + bounds.height / 2 + window.scrollY;
    draw(1, left, top);
    draw(2, left, top);
    draw(4);
  });
  window.addEventListener("scroll", schedule, { passive: true });
  window.addEventListener("resize", schedule);
  document.fonts.ready.then(schedule);
  document.fonts.addEventListener("loadingdone", schedule);

  function watchResolution() {
    window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
      .addEventListener("change", () => {
        watchResolution();
        schedule();
      }, { once: true });
  }
  watchResolution();

  return { exports, draw };
}

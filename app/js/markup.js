// The application's markup, in one place: the page that hosts it (a web
// site, or the bare page of a local install) gives it an element and
// `mount` in app.js puts this inside.
//
// Nothing here is about a particular host: no words about a site, no
// numbers, no links. A host adds what it wants to the bar at the top
// through `options.top`.

export const APP_HTML = `
<div class="stripe" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i><i></i></div>

<header class="top">
  <a class="brand" href="./" aria-label="anycube">
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <rect x="1" y="1" width="6.4" height="6.4" rx="1.4" class="m0"/><rect x="8.8" y="1" width="6.4" height="6.4" rx="1.4" class="m1"/><rect x="16.6" y="1" width="6.4" height="6.4" rx="1.4" class="m2"/>
      <rect x="1" y="8.8" width="6.4" height="6.4" rx="1.4" class="m3"/><rect x="8.8" y="8.8" width="6.4" height="6.4" rx="1.4" class="m4"/><rect x="16.6" y="8.8" width="6.4" height="6.4" rx="1.4" class="m5"/>
      <rect x="1" y="16.6" width="6.4" height="6.4" rx="1.4" class="m1"/><rect x="8.8" y="16.6" width="6.4" height="6.4" rx="1.4" class="m5"/><rect x="16.6" y="16.6" width="6.4" height="6.4" rx="1.4" class="m3"/>
    </svg>
    <span>cube<b>n</b>x<b>n</b>x<b>n</b></span>
  </a>
  <div class="top-extra" id="top-extra"></div>
</header>

<main class="bench" aria-label="Solver">
  <section class="stage-col" aria-label="The cube">
    <div class="stage cube-host" id="stage">
      <canvas id="cube" tabindex="0" role="img" aria-label="The cube in three dimensions. Drag a layer to turn it, drag beside the cube or use the arrow keys to look around."></canvas>
      <div class="cam-frame" id="camera-frame" hidden></div>
      <p class="stage-size" id="stage-size">3x3x3</p>
      <div class="stage-tools">
        <button type="button" class="tool" id="arrows" aria-pressed="true" title="Show the next move as arrows on the cube">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12h14M13 6l6 6-6 6"/></svg><span>Arrows</span>
        </button>
        <button type="button" class="tool" id="reset-view" title="Back to the usual view of the cube">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 1 0 3-6.2M4 4v5h5"/></svg><span>View</span>
        </button>
      </div>
      <p class="next" id="next" hidden><span>next</span><b id="next-move"></b></p>
      <div class="status" id="status" role="status" aria-live="polite" hidden></div>
    </div>

    <div class="transport" role="group" aria-label="Playback">
      <div class="keys">
        <button type="button" class="key" id="to-start" aria-label="Back to the scrambled cube" title="Back to the start">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 5v14M19 6l-9 6 9 6z"/></svg>
        </button>
        <button type="button" class="key" id="step-back" aria-label="One move back" title="One move back (left arrow)">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M16 6l-9 6 9 6z"/></svg>
        </button>
        <button type="button" class="key play" id="play" aria-label="Play" title="Play or pause (space)">
          <svg viewBox="0 0 24 24" aria-hidden="true" class="icon-play"><path d="M8 5l12 7-12 7z"/></svg>
          <svg viewBox="0 0 24 24" aria-hidden="true" class="icon-pause"><path d="M7 5h4v14H7zM13 5h4v14h-4z"/></svg>
        </button>
        <button type="button" class="key" id="step-on" aria-label="One move on" title="One move on (right arrow)">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 6l9 6-9 6z"/></svg>
        </button>
        <button type="button" class="key" id="to-end" aria-label="On to the solved cube" title="On to the end">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18 5v14M5 6l9 6-9 6z"/></svg>
        </button>
      </div>
      <p class="counter" aria-live="off"><span id="at">0</span><span class="of">/<span id="total">0</span></span></p>
      <input type="range" id="scrub" min="0" max="0" value="0" step="1" aria-label="Position in the solution">
      <div class="speed" role="radiogroup" aria-label="Speed">
        <button type="button" role="radio" aria-checked="false" data-speed="0.5">½×</button>
        <button type="button" role="radio" aria-checked="true" data-speed="1">1×</button>
        <button type="button" role="radio" aria-checked="false" data-speed="2">2×</button>
        <button type="button" role="radio" aria-checked="false" data-speed="3">3×</button>
        <button type="button" role="radio" aria-checked="false" data-speed="6">6×</button>
        <button type="button" role="radio" aria-checked="false" data-speed="12">12×</button>
      </div>
    </div>
  </section>

  <section class="panel" aria-label="The cube to solve">
    <div class="size-row">
      <div class="sizes" id="sizes" role="radiogroup" aria-label="Size of the cube"></div>
      <label class="custom" title="Any size">
        <span class="sr">Another size</span>
        <input type="number" id="custom-size" inputmode="numeric" min="2" placeholder="N">
      </label>
    </div>

    <div class="tabs" role="tablist" aria-label="How to enter the cube">
      <button type="button" role="tab" id="tab-scramble" aria-controls="panel-scramble" aria-selected="true">Scramble</button>
      <button type="button" role="tab" id="tab-paint" aria-controls="panel-paint" aria-selected="false" tabindex="-1">Paint</button>
      <button type="button" role="tab" id="tab-camera" aria-controls="panel-camera" aria-selected="false" tabindex="-1">Camera</button>
      <button type="button" role="tab" id="tab-moves" aria-controls="panel-moves" aria-selected="false" tabindex="-1" class="narrow-only">Moves</button>
    </div>

    <div class="panels">
      <div role="tabpanel" id="panel-scramble" aria-labelledby="tab-scramble" class="tabpanel">
        <label class="sr" for="scramble">Scramble</label>
        <textarea id="scramble" rows="3" spellcheck="false" autocapitalize="off" autocomplete="off" autocorrect="off" placeholder="R U R' U' F2 D L' B …"></textarea>
        <div class="row">
          <button type="button" class="button" id="random">Random scramble</button>
          <button type="button" class="button" id="undo" title="Take back the last move">Undo</button>
          <button type="button" class="button" id="clear">Clear</button>
        </div>
        <p class="note" id="scramble-kind"></p>
        <p class="note wide-only" id="notation"></p>
      </div>

      <div role="tabpanel" id="panel-paint" aria-labelledby="tab-paint" class="tabpanel" hidden>
        <p class="note" id="paint-note">Pick a colour, then tap or drag over the stickers of the cube.</p>
        <div id="paint" class="paint"></div>
        <div class="row" id="paint-buttons">
          <button type="button" class="button" id="paint-reset">Solved cube</button>
          <button type="button" class="button" id="paint-blank">Blank cube</button>
        </div>
        <p class="note" id="paint-large" hidden></p>
      </div>

      <div role="tabpanel" id="panel-camera" aria-labelledby="tab-camera" class="tabpanel" hidden>
        <div id="camera-intro">
          <p class="note">Show the six faces to the camera, one by one, inside a grid; then correct any colour that came out wrong.</p>
          <p class="private"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2l8 3v6c0 5-3.4 9.3-8 11-4.6-1.7-8-6-8-11V5z"/></svg>The picture never leaves your browser. Nothing is recorded or uploaded.</p>
          <button type="button" class="button primary" id="camera-start">Start the camera</button>
        </div>
        <div id="camera-scan" class="cam-scan" hidden>
          <div id="camera-side" class="cam-side"></div>
          <div id="scan-slot-narrow" class="cube-host scan-slot-narrow"></div>
        </div>
        <p class="note" id="camera-large" hidden></p>
      </div>

      <!-- On a narrow screen the solution lives here; app.js moves it. -->
      <div role="tabpanel" id="panel-moves" aria-labelledby="tab-moves" class="tabpanel" hidden></div>
    </div>

    <!-- The lower part of the panel: the solution, or, while the camera reads a cube, the cube being filled in. -->
    <div class="lower" id="panel-lower">
      <section class="solution" id="solution" aria-label="Solution">
        <p id="summary" class="summary"></p>
        <button type="button" class="button small" id="copy" hidden>Copy</button>
        <div id="moves" class="moves"></div>
        <p class="placeholder" id="placeholder"></p>
      </section>
      <div class="cube-host scan-slot" id="scan-slot"></div>
    </div>

    <p class="valid" id="valid" role="status" aria-live="polite"></p>

    <div class="go">
      <div class="modes" id="mode-field" role="radiogroup" aria-label="Search">
        <button type="button" role="radio" aria-checked="false" data-mode="fast" title="A fraction of the time, about a move longer">Fast</button>
        <button type="button" role="radio" aria-checked="true" data-mode="normal" title="The solver's defaults">Normal</button>
      </div>
      <button type="button" class="button primary large" id="solve">Solve</button>
    </div>
  </section>
</main>
`;

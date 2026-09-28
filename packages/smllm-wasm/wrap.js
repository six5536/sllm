// The typed layer over the raw string API (PLAN-006 D6-3): objects in and
// out, and an object-based host. `raw` is a build's wasm-bindgen `Engine`.

const parse = (text) => JSON.parse(text);

/** The raw host the wasm calls: strings in, strings out; defaults filled. */
function adapt(host) {
  return {
    supports: (kind) => Boolean(host.supports?.(kind)),
    check: (kind, params, env, cwd) =>
      host.check ? Boolean(host.check(kind, parse(params), parse(env), cwd)) : false,
    run: (kind, params, env, cwd) =>
      host.run ? (host.run(kind, parse(params), parse(env), cwd) ?? "") : `${kind} is not supported`,
    read: (file) => host.read?.(file) ?? undefined,
    isMatch: (pattern, value) =>
      host.isMatch ? host.isMatch(pattern, value) : new RegExp(pattern).test(value),
    now: () => (host.now ? host.now() : Date.now()),
    random: () => (host.random ? host.random() : crypto.getRandomValues(new Uint32Array(1))[0]),
    history: (machine, id, entry) => {
      host.history?.(machine, id, parse(entry));
    },
  };
}

/** The typed `Engine` class over a build's raw `Engine`. */
export function wrap(Raw) {
  return class Engine {
    #raw;

    constructor(compiled, host = {}) {
      const json = typeof compiled === "string" ? compiled : JSON.stringify(compiled);
      this.#raw = new Raw(json, adapt(host));
    }

    unsupported() {
      return parse(this.#raw.unsupported());
    }

    bind(harness, hostSession, cwd = "") {
      return parse(this.#raw.bind(harness, hostSession, cwd));
    }

    view(key) {
      return parse(this.#raw.view(key));
    }

    events(key) {
      return parse(this.#raw.events(key));
    }

    status(key) {
      return parse(this.#raw.status(key));
    }

    fire(key, event, params = {}) {
      return parse(this.#raw.fire(key, event, JSON.stringify(params)));
    }

    stop(key, stopHookActive = false) {
      return parse(this.#raw.stop(key, stopHookActive));
    }

    promptSubmitted(key) {
      this.#raw.promptSubmitted(key);
    }

    exportState() {
      return parse(this.#raw.exportState());
    }

    importState(state) {
      this.#raw.importState(typeof state === "string" ? state : JSON.stringify(state));
    }

    /** Free the wasm memory now, rather than when collected. */
    free() {
      this.#raw.free();
    }

    [Symbol.dispose]() {
      this.free();
    }
  };
}

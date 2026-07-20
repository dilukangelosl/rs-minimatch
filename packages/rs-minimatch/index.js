// Full API surface, matching `require('minimatch')`: the default export is
// the `minimatch` function itself, with everything else (Minimatch, filter,
// match, braceExpand, defaults, escape, unescape, GLOBSTAR, sep) attached
// as properties, same shape the real package uses. All matching logic is
// in the native addon; `filter`/`defaults` here are just closures over it.
'use strict'

const native = require('./native.js')

const minimatch = (path, pattern, options) => native.minimatch(path, pattern, options)

minimatch.Minimatch = native.Minimatch
minimatch.match = native.match
minimatch.braceExpand = native.braceExpand
minimatch.escape = native.escape
minimatch.unescape = native.unescape
minimatch.sep = process.platform === 'win32' ? '\\' : '/'
minimatch.GLOBSTAR = Symbol('globstar **')

minimatch.filter = (pattern, options) => (p) => minimatch(p, pattern, options)

minimatch.defaults = (def) => {
  if (!def || typeof def !== 'object' || !Object.keys(def).length) {
    return minimatch
  }
  const ext = (o) => Object.assign({}, def, o || {})

  const m = (path, pattern, options) => minimatch(path, pattern, ext(options))
  m.Minimatch = class extends minimatch.Minimatch {
    constructor(pattern, options) {
      super(pattern, ext(options))
    }
  }
  m.match = (list, pattern, options) => minimatch.match(list, pattern, ext(options))
  m.braceExpand = (pattern, options) => minimatch.braceExpand(pattern, ext(options))
  m.escape = minimatch.escape
  m.unescape = minimatch.unescape
  m.filter = (pattern, options) => minimatch.filter(pattern, ext(options))
  m.defaults = (options) => minimatch.defaults(ext(options))
  m.sep = minimatch.sep
  m.GLOBSTAR = minimatch.GLOBSTAR
  return m
}

module.exports = minimatch

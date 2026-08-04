;; Realistic, correct Hy. Every shape this package's rule looks at appears
;; here, in the order a competent author would write it, and none of it earns a
;; finding. The companion `dangerous.hy` is the same shapes written wrongly.
;;
;; The point of this file is the *denominator*: the corpus test asserts that it
;; contains handler chains for the rule to judge. A clean sweep over a file
;; with no multi-clause `try` would prove nothing at all.

(import os json sys)
(import pathlib [Path])

;; Narrow to broad: the ordinary correct shape.
(defn load-config [path]
  (try
    (json.loads (.read-text (Path path)))
    (except [e FileNotFoundError]
      (print "no config at" path)
      {})
    (except [e json.JSONDecodeError]
      (print "bad json" e)
      {})
    (except [e OSError]
      (print "io error" e)
      {})))

;; A tuple of types, then a broader clause. Neither type is covered above.
(defn fetch [key store]
  (try
    (get store key)
    (except [e [KeyError IndexError]]
      None)
    (except [e Exception]
      (print "unexpected" e)
      None)))

;; Sibling types under one parent: neither covers the other.
(defn convert [text]
  (try
    (int text)
    (except [e ValueError]
      0)
    (except [e TypeError]
      0)))

;; A project exception class beside a builtin. This layer cannot see what
;; `ConfigError` inherits from, so it must not guess in either direction.
(defclass ConfigError [Exception])

(defn strict-load [path]
  (try
    (load-config path)
    (except [e ConfigError]
      (sys.exit 2))
    (except [e OSError]
      (sys.exit 1))))

;; `Exception` before a project class is deliberately NOT reported: the class
;; may derive from `BaseException` directly, in which case this clause is live.
(defn tolerant [path]
  (try
    (load-config path)
    (except [e Exception]
      (print "generic"))
    (except [e ConfigError]
      (print "specific"))))

;; A single-clause `try` can never earn a finding, and `else`/`finally` are not
;; except clauses.
(defn read-once [path]
  (try
    (.read-text (Path path))
    (except [e OSError]
      "")
    (else
      (print "ok"))
    (finally
      (print "done"))))

;; A bare `except` that is last shadows nothing. Reporting its breadth is the
;; sibling package's `hy-bare-except`, not this rule's business.
(defn best-effort [thunk]
  (try
    (thunk)
    (except [e ValueError]
      None)
    (except []
      None)))

;; A macro template. Nothing inside a Hy quasiquote is code as far as this
;; package is concerned, so even a genuinely shadowed chain here is not
;; reported.
(defmacro with-logging [#* body]
  `(try
     ~@body
     (except [e Exception]
       (print "failed"))
     (except [e ValueError]
       (print "value"))))

;; `#(...)` is a tuple, a self-evaluating constant rather than a call.
(setv shapes #(1 2 3))

;; A dotted exception type this layer cannot resolve.
(defn socket-read [sock]
  (try
    (.recv sock 1024)
    (except [e socket.timeout]
      b"")
    (except [e OSError]
      b"")))

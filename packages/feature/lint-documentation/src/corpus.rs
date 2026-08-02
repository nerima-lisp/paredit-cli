//! The realistic-correct corpus every rule in this package is swept over.
//!
//! Embedded as string constants rather than shipped as `.lisp`/`.el`/`.clj`
//! files, for a reason specific to this repository: `flake.nix` configures
//! treefmt to format **every** Lisp-family file it finds with `paredit edit
//! format` itself, excluding only `tests/fixtures/*`, `fuzz/corpus/*` and
//! `fuzz/artifacts/*`. A corpus file under `packages/` would therefore be
//! rewritten by `nix flake check`, silently changing the exact bytes — line
//! widths, comment placement — that the sweep asserts on. `tests/fixtures/` is
//! excluded from treefmt for the same reason and is out of scope for this
//! change, so the corpus lives here, in a form no formatter touches.
//!
//! Each of these is ordinary, idiomatic, *correct* code. Every rule in this
//! package must stay silent on all of it; anything that fires is a false
//! positive. See [`crate::fixture_sweep`] for the sweep itself and for the
//! two guards that keep "no findings" from being a vacuous result.

/// Idiomatic Common Lisp: documented and undocumented definitions,
/// worked examples with correct arity, `&optional`/`&rest`/`&key` lambda
/// lists, attributed task markers in six notations, macro templates that
/// *build* definitions, and quoted data that merely looks like definitions.
pub const COMMON_LISP: &str = r##";;;; realistic-correct.lisp — idiomatic, well-documented Common Lisp.
;;;;
;;;; This file is the false-positive corpus for
;;;; `paredit-feature-lint-documentation`. Every rule in that package is run
;;;; over it and every rule must stay silent.
;;;;
;;;; It is deliberately dense in the shapes those rules examine: documented and
;;;; undocumented definitions, worked examples with correct arity, `&optional`
;;;; / `&rest` / `&key` lambda lists, attributed task markers in six notations,
;;;; and macro templates that *build* definitions. Anything here that produces
;;;; a finding is a false positive, and the sweep says which.

(defpackage #:app.retry
  (:use #:cl)
  (:export #:retry #:scale #:total #:render #:with-timeout #:*timeout*)
  (:documentation "Retrying, scaling, and rendering — the application's public interface."))

(in-package #:app.retry)

;;; Configuration

(defparameter *timeout* 30
  "Seconds to wait for the server before giving up.")

(defvar *cache* nil
  "Memoized results, keyed by request id. Cleared by `reset-cache'.")

(defconstant +max-retries+ 5
  "The largest number of attempts `retry' will make.")

(defvar *undocumented-on-purpose* nil)

;;; Arithmetic

(defun scale (x factor)
  "Return X scaled by FACTOR.

Example: (scale 3 2) => 6"
  (* x factor))

(defun offset (x &optional (by 1))
  "Return X moved by BY, which defaults to 1.

Both of these are correct calls:

  (offset 3)
  (offset 3 10)"
  (+ x by))

(defun total (&rest numbers)
  "Sum NUMBERS.

Example: (total 1 2 3 4 5) => 15
An empty call is fine too: (total) => 0"
  (apply #'+ numbers))

(defun clamp (value low high)
  "Return VALUE confined to the range LOW..HIGH.

Example: (clamp 12 0 10) => 10"
  (max low (min value high)))

;;; Rendering

(defun render (object &key stream pretty)
  "Write OBJECT to STREAM.

PRETTY is currently accepted and ignored. Keyword arguments this function does
not name are forwarded by callers, so all of these are legal:

  (render x)
  (render x :stream s)
  (render x :stream s :pretty t)"
  (declare (ignore pretty))
  (print object stream))

(defun render-all (objects &rest options &key &allow-other-keys)
  "Write each of OBJECTS, forwarding OPTIONS to `render'.

Example: (render-all list :stream s :pretty t :depth 3)"
  (dolist (object objects)
    (apply #'render object options)))

;;; Control

(defun retry (n thunk)
  "Attempt THUNK up to N times.

Returns the first successful value, re-signalling the last condition if every
attempt fails. N is capped at `+max-retries+'.

Example: (retry 3 (lambda () (fetch)))"
  (loop repeat (min n +max-retries+)
        do (handler-case (return (funcall thunk))
             (error (condition) (setf *cache* condition)))
        finally (error "every attempt failed")))

(defmacro with-timeout ((seconds) &body body)
  "Run BODY with a SECONDS deadline.

Example: (with-timeout (5) (fetch) (parse))"
  `(let ((*timeout* ,seconds))
     ,@body))

(defmacro when-let ((variable value) &body body)
  "Bind VARIABLE to VALUE and run BODY when VALUE is non-nil.

Example: (when-let (x (lookup k)) (use x) (log x))"
  `(let ((,variable ,value))
     (when ,variable ,@body)))

;;; A function whose body is a single string is *returning* it, not
;;; documenting itself. Neither the width rule nor the example rule may read it.

(defun greeting ()
  "hello")

(defun banner ()
  "================================================================================================")

;;; CLOS

(defclass request ()
  ((id :initarg :id :reader request-id :documentation "The request's unique id.")
   (body :initarg :body :reader request-body))
  (:documentation "One inbound request."))

(define-condition retry-exhausted (error)
  ((attempts :initarg :attempts :reader retry-exhausted-attempts))
  (:report (lambda (condition stream)
             (format stream "gave up after ~D attempts"
                     (retry-exhausted-attempts condition))))
  (:documentation "Signalled when every attempt has failed."))

(defgeneric handle (request)
  (:documentation "Handle REQUEST and return its response."))

(defmethod handle ((request request))
  "Handle a plain REQUEST by echoing its body."
  (request-body request))

;;; Macro templates. The definitions below are *built*, not defined, so the
;;; docstrings in them are not this file's to judge — including the deliberately
;;; wrong example and the deliberately enormous summary line.

(defmacro define-accessor (name)
  "Define an accessor called NAME."
  `(defun ,name (object)
     "Example: (whatever 1 2 3 4 5 6 7 8 9)"
     (slot-value object ',name)))

(defmacro define-wide (name)
  "Define a function called NAME with a very wide docstring."
  `(defun ,name ()
     "This summary line is deliberately far longer than any reasonable limit so that it would be reported if this template were ever read as a definition rather than as the list data it is."
     nil))

;;; Quoted data. `(defpackage …)` here is a list of symbols, not a declaration.

(defvar *forms*
  '((defpackage :not-a-real-package (:use :cl))
    (defun not-a-real-function (x) "Example: (not-a-real-function 1 2 3)" x))
  "Example forms, as data, for the test suite to read.")

;;; Task markers, each attributed in a different notation. None may be reported.

;; TODO(ada): memoize this once #412 lands.
(defun expensive (x)
  "Compute the expensive thing for X."
  (identity x))

;; FIXME[bruno]: the fallback path is untested.
(defun fallback (x)
  "Return X unchanged."
  x)

;; HACK: works around https://example.com/bugs/9 until the upstream fix ships.
(defun workaround (x)
  "Return X unchanged."
  x)

;; XXX: revisit after 2026-12-01.
(defun scheduled (x)
  "Return X unchanged."
  x)

;; BUG: see PROJ-88 — the retry budget is off by one.
(defun budget ()
  "Return the retry budget."
  +max-retries+)

;; TODO: @carla owns the rewrite of this dispatch table.
(defun dispatch (key)
  "Return the handler registered for KEY."
  (gethash key *cache*))

;;; Comments that merely mention a marker are not markers.

;; The TODO list for this module lives in NOTES.md.
;; TODOs are tracked in the issue tracker, not here.
;; This function is buggy in the sense that it is slow, not wrong.

(defun documented-elsewhere (x)
  "Return X unchanged."
  x)
"##;

/// Idiomatic Emacs Lisp, headers and all. Only
/// `todo-fixme-no-attribution` is declared for this dialect, so what this
/// really pins is that a conventional `;;;` section header and a package
/// preamble produce nothing.
pub const EMACS_LISP: &str = r##";;; realistic-correct.el --- Idiomatic Emacs Lisp -*- lexical-binding: t -*-

;; Copyright (C) 2026 The paredit-cli authors
;; Author: The paredit-cli authors
;; Package-Requires: ((emacs "27.1"))

;;; Commentary:

;; The Emacs Lisp half of the false-positive corpus for
;; `paredit-feature-lint-documentation'.  Only `todo-fixme-no-attribution'
;; is declared for this dialect — the docstring rules encode Common Lisp's own
;; grammar — so what this file is really testing is that a conventional Emacs
;; Lisp file header, with its `;;;' section markers and its package headers,
;; produces no findings.

;;; Code:

(defgroup app nil
  "The application."
  :group 'tools)

(defcustom app-timeout 30
  "Seconds to wait for the server before giving up."
  :type 'integer
  :group 'app)

(defvar app--cache nil
  "Memoized results, keyed by request id.")

(defun app-scale (x factor)
  "Return X scaled by FACTOR."
  (* x factor))

(defun app-clamp (value low high)
  "Return VALUE confined to the range LOW..HIGH."
  (max low (min value high)))

;;;###autoload
(defun app-run (&optional buffer)
  "Run the application, displaying results in BUFFER."
  (interactive)
  (with-current-buffer (or buffer (current-buffer))
    (app-scale 1 2)))

;; TODO(ada): drop this once #412 lands.
(defun app--legacy (x)
  "Return X unchanged."
  x)

;; FIXME: see PROJ-88 — this path is untested.
(defun app--fallback (x)
  "Return X unchanged."
  x)

;; HACK: works around https://debbugs.gnu.org/1234 until Emacs 30.
(defun app--workaround (x)
  "Return X unchanged."
  x)

;; XXX: revisit after 2026-12-01.
(defun app--scheduled (x)
  "Return X unchanged."
  x)

;; The TODO list for this package lives in the NEWS file.

(provide 'app)
;;; realistic-correct.el ends here
"##;

/// Idiomatic Clojure. `ns` is deliberately not read by
/// `missing-package-docstring`, so this pins that a documented namespace and
/// a set of attributed markers both stay silent.
pub const CLOJURE: &str = r##"(ns app.retry
  "Retrying, scaling and rendering — the application's public interface."
  (:require [clojure.string :as str]))

;; The Clojure half of the false-positive corpus for
;; paredit-feature-lint-documentation. Only todo-fixme-no-attribution is
;; declared for this dialect: `ns` is deliberately not read by
;; missing-package-docstring, because its docstring has a metadata spelling
;; (^{:doc "..."}) that reading wrongly would report a documented namespace.

(def ^:private max-retries
  "The largest number of attempts retry will make."
  5)

(defn scale
  "Return x scaled by factor.

  Example: (scale 3 2) => 6"
  [x factor]
  (* x factor))

(defn total
  "Sum the numbers.

  Example: (total 1 2 3 4 5) => 15"
  [& numbers]
  (apply + numbers))

(defn render
  "Write object to the writer named by opts."
  [object & {:keys [stream pretty]}]
  (binding [*out* (or stream *out*)]
    (print object)))

;; TODO(ada): memoize this once #412 lands.
(defn expensive [x] x)

;; FIXME[bruno]: the fallback path is untested.
(defn fallback [x] x)

;; HACK: works around https://clojure.atlassian.net/browse/CLJ-1234.
(defn workaround [x] x)

;; XXX: revisit after 2026-12-01.
(defn scheduled [x] x)

;; BUG: see PROJ-88 — the retry budget is off by one.
(defn budget [] max-retries)

;; The TODO list for this namespace lives in doc/notes.md.

(defn documented-elsewhere [x] x)
"##;

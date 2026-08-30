;; The dangerous twin of `clean.hy`: the same shapes written so that an earlier
;; clause kills a later one. Each `try` here earns exactly the findings the
;; corpus test pins, and the shapes are the ones adjudicated as true positives
;; during the third-party audit.
;;
;; The audit's single real-world finding was this file's `import-order` shape,
;; found in `atisharma/hyjinx`: `ImportError` before `ModuleNotFoundError`.
(import os json sys)

;; A supertype first. `ValueError` never runs.
(defn broad-first
      [text]
      (try (int text)
           (except [e Exception] (print "generic"))
           (except [e ValueError] (print "never runs"))))

;; The shape found in real third-party code: `ModuleNotFoundError` is a
;; subclass of `ImportError`, so the second clause is dead.
(defn import-order
      [name]
      (try (__import__ name)
           (except [e ImportError] None)
           (except [e ModuleNotFoundError] None)))

;; The same type twice.
(defn duplicated
      [store key]
      (try (get store key) (except [e KeyError] 1) (except [e KeyError] 2)))

;; A bare clause first kills both clauses after it.
(defn bare-first
      [thunk]
      (try (thunk)
           (except [] None)
           (except [e ValueError] None)
           (except [e OSError] None)))

;; `BaseException` covers even a class this layer has never heard of.
(defclass AppError [Exception])

(defn base-first
      [thunk]
      (try (thunk) (except [e BaseException] None) (except [e AppError] None)))

;; Every type of a tuple already covered: `KeyError` and `IndexError` are both
;; `LookupError`s.
(defn tuple-covered
      [store key]
      (try (get store key)
           (except [e LookupError] 1)
           (except [e [KeyError IndexError]] 2)))

;; `IOError` is not a subclass of `OSError`, it *is* `OSError`, so this is a
;; duplicate rather than a narrowing.
(defn alias-duplicate
      [path]
      (try (open path) (except [e OSError] None) (except [e IOError] None)))

;; Transitivity: `UnicodeDecodeError` -> `UnicodeError` -> `ValueError`.
(defn transitive
      [raw]
      (try (.decode raw "utf-8")
           (except [e ValueError] "")
           (except [e UnicodeDecodeError] "")))

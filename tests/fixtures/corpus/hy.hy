;; Hy: Python semantics under Lisp syntax. Flat-pair `cond`, `setv` rather
;; than `let`-heavy binding, dotted attribute access, `lfor` comprehensions,
;; and `~`/`~@` for macro unquoting.
(import json pathlib [Path])

(setv DEFAULTS {"retries" 3 "timeout" 0.5})

(defn classify
      [n]
      "Name the magnitude of n."
      (cond
        (<
          n
          0)
        "negative"
        (=
          n
          0)
        "zero"
        (>
          n
          100)
        "large"
        True
        "positive"))

(defclass Account []
  (defn __init__
        [self id [balance 0]]
        (setv self.id id)
        (setv self.balance balance))
  (defn transfer
        [self other amount]
        (when (< self.balance amount)
          (raise (ValueError (.format "insufficient funds in {}" self.id))))
        (-= self.balance amount)
        (+= other.balance amount)
        #(self other)))

(defn summarise
      [accounts]
      (setv total (sum (lfor account accounts account.balance)))
      (setv labels
            (lfor account
                  accounts
                  :if
                  account.balance
                  (.format "{}:{}" account.id account.balance)))
      (.format "{} (total {})" (.join ", " labels) total))

(defn load-config
      [path]
      (with [handle (open (Path path) "r")] (json.load handle)))

(defn retry
      [callback attempts]
      (for [attempt (range attempts)]
           (try (return (callback))
                (except [error RuntimeError] (print "attempt failed:" error))))
      None)

(defmacro unless [test form]
  `(when (not ~test)
     ~form))

(defn main
      []
      (setv accounts [(Account "a" 10) (Account "b" 0)])
      (print (summarise accounts))
      (unless (get DEFAULTS "retries")
        (print "no retries configured")))

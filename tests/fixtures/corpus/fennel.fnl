;; Fennel: Lua semantics under Lisp syntax. `fn`/`lambda` definitions, `local`
;; and `var` rather than `def`, `if` as a multi-branch conditional, sequential
;; and associative table literals, `each`/`for` iteration, and the `#(...)`
;; hashfn shorthand with `$1`.
(local {: view} (require :fennel))

(local default-options {:retries 3 :timeout 0.5})

(local double #(* 2 $1))

(fn classify
    [n]
    "Name the magnitude of n."
    (if (< n 0)
        :negative
        (= n 0)
        :zero
        (> n 100)
        :large
        :positive))

(fn transfer
    [from to amount]
    (when (< from.balance amount)
      (error (string.format "insufficient funds in %s" from.id)))
    (set from.balance (- from.balance amount))
    (set to.balance (+ to.balance amount))
    (values from to))

(fn summarise
    [accounts]
    (var total 0)
    (local labels [])
    (each [_ account (ipairs accounts)]
          (set total (+ total account.balance))
          (when (> account.balance 0)
            (table.insert labels (.. account.id ":" account.balance))))
    (.. (table.concat labels ", ") " (total " total ")"))

(lambda retry
  [callback attempts]
  (var result nil)
  (for [attempt 1 attempts]
       (when (= result nil)
         (let [(ok value) (pcall callback)]
           (when ok
             (set result value)))))
  result)

(macro unless
       [condition ...]
       `(when (not ,condition)
          ,...))

(fn report
    [items]
    (each [_ item (ipairs items)]
          (unless item.hidden
            (print (view item)))))

{: classify : double : report : summarise : transfer}

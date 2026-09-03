# Janet: `#` line comments, `@` for mutable literals, keyword keys, `|` short
# function literals, `;` splicing, `~` quasiquote with `,` unquote, and a PEG
# grammar as ordinary data.

(import spork/json :as json)

(def default-options
  {:retries 3
   :timeout 0.5
   :tags @[:corpus :layout]})

(defn classify
  "Name the magnitude of n."
  [n]
  (cond
    (< n 0) :negative
    (= n 0) :zero
    (> n 100) :large
    :positive))

(defn transfer [from to amount]
  (when (< (from :balance) amount)
    (errorf "insufficient funds in %j" (from :id)))
  [(merge from {:balance (- (from :balance) amount)})
   (merge to {:balance (+ (to :balance) amount)})])

(defn summarise [accounts]
  (let [total (sum (map |($ :balance) accounts))
        labels (map (fn [account]
                      (string (account :id) ":" (account :balance)))
                    accounts)]
    (string/join [;labels (string "total " total)] ", ")))

(def word-grammar
  (peg/compile
    ~{:word (some (range "az" "AZ"))
      :sep (some (set " \t"))
      :main (some (+ :word :sep))}))

(defn walk [tree]
  (var acc @[])
  (each node tree
    (if (indexed? node)
      (array/concat acc (walk node))
      (array/push acc node)))
  acc)

(defmacro unless [condition & body]
  ~(if ,condition nil (do ,;body)))

(defn main [& args]
  (each arg args
    (unless (empty? arg)
      (print (classify (scan-number arg))))))

;;;; LFE: a module of pattern-matching clauses over Erlang terms.
;;;;
;;;; Idiomatic LFE, not Common Lisp in an .lfe file: multi-clause `defun`
;;;; bodies, tuple literals under quasiquote, `cond` ending in `'true`, and
;;;; module-qualified calls with a colon.
(defmodule corpus
           (doc "A layout corpus fixture.")
           (export (start 0) (sum 1) (classify 1) (summarise 1))
           (export (init 1) (handle_call 3) (handle_cast 2)))

(defun start ()
  "Start the server under its module name."
  (gen_server:start_link `#(local ,(MODULE)) (MODULE) '() '()))

(defun sum (('()) 0)
  (((cons head tail)) (+ head (sum tail))))

(defun classify (n)
  (cond
    ((< n 0) 'negative)
    ((=:= n 0) 'zero)
    ((> n 100) 'large)
    ('true 'positive)))

(defun summarise (accounts)
  (let* ((total
          (lists:foldl
           (lambda (account acc)
             (+ (element 2 account) acc))
           0
           accounts))
         (labels (lists:map
                  (lambda (account)
                    (io_lib:format "~s:~p"
                                   (list (element 1 account)
                                         (element 2 account))))
                  accounts)))
    (lists:flatten (list labels (io_lib:format " (total ~p)" (list total))))))

(defun init (_args)
  `#(ok ,(maps:new)))

(defun handle_call ((`#(get ,key) _from state)
                    `#(reply ,(maps:get key state 'undefined) ,state))
  ((`#(put ,key ,value) _from state) `#(reply ok ,(maps:put key value state)))
  ((_message _from state) `#(reply #(error unknown) ,state)))

(defun handle_cast (_message state)
  `#(noreply ,state))

(defmacro unless (test body)
  `(if ,test
       'ok
       ,body))

#lang racket/base
;; Realistic, *correct* Racket. Every head this package anchors on appears
;; here, several of them in the near-miss shapes the rules must decline, and
;; the whole file must earn zero findings.
;;
;; This file is checked by `racket` itself in the report accompanying the
;; package, not only by the linter.
(require racket/match racket/list
  racket/contract)

(provide (contract-out
          [classify (-> any/c symbol?)]
          [tally (-> (listof exact-integer?) exact-integer?)]) render-all)

;; A `match` whose catch-all is last, which is the overwhelmingly common and
;; entirely correct shape.
(define
 (classify value)
 (match
  value
  [(? number?) 'number]
  [(? string?) 'string]
  [(list _ ...) 'list]
  [_ 'other]))

;; A guarded clause is not a catch-all: the guard can fail, so the clause
;; below it is live.
(define
 (bucket n)
 (match
  n
  [x #:when (< x 0) 'negative]
  [x #:when (zero? x) 'zero]
  [0 'unreachable-but-not-by-catch-all]
  [_ 'positive]))

;; Quoted symbols are literals, not bindings.
(define (direction->delta d) (match d ['north -1] ['south 1] [_ 0]))

;; `match-lambda` with the catch-all last.
(define
 describe
 (match-lambda
  [(list 'ok value) value]
  [(list 'error message) message]
  [_ "unknown"]))

;; A `begin0` that genuinely sequences: it returns the first form's value and
;; still runs the cleanup.
(define (pop! box-cell) (begin0 (unbox box-cell) (set-box! box-cell '())))

;; A `case-lambda` that genuinely dispatches on arity.
(define
 tally
 (case-lambda
  [(xs) (tally xs 0)]
  [(xs start) (for/fold ([total start]) ([x (in-list xs)]) (+ total x))]))

;; A `parameterize` that genuinely rebinds.
(define (render-all items)
  (parameterize ([current-output-port (open-output-string)])
    ;; `for` iterates for effect and allocates no container: the recommendation,
    ;; not the defect.
    (for ([item (in-list items)])
      (displayln (classify item)))
    ;; A comprehension in the *last* body position is the result, which is what
    ;; comprehensions are for.
    (for/list ([item (in-list items)])
      (classify item))))

;; A comprehension bound to a name has its value read.
(define
 (summarise items)
 (let ([labels (for/list
                ([item (in-list items)])
                (classify item))]
       [count (length items)])
   (when (positive? count)
     (displayln count))
   labels))

;; A named `let`, whose body starts one position later than an ordinary one.
(define
 (drain queue)
 (let loop
   ([remaining queue] [seen '()])
   (unless (null? remaining)
     (displayln (car remaining)))
   (if (null? remaining) (reverse seen)
     (loop (cdr remaining) (cons (car remaining) seen)))))

;; `let*`, `letrec` and a plain `lambda`, all with their comprehension last.
(define
 transform
 (lambda (items)
   (let* ([n (length items)]
          [scaled (for/list ([i (in-range n)]) (* i 2))])
     (letrec
      ([go
        (lambda (xs)
          (if (null? xs) '()
            (cons (car xs) (go (cdr xs)))))])
      (go scaled)))))

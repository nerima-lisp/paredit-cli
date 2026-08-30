#lang racket/base
;;; Realistic, correct Racket that exercises every head this package anchors
;;; on, in the bracket-heavy style the Racket style guide asks for.
(require racket/list racket/string)

(provide tokenize summarize)

;; `let*` that uses its sequential scope, written with brackets.
(define (tokenize text)
        (let* ([trimmed (string-trim text)]
               [parts (string-split trimmed)]
               [kept (filter non-empty-string? parts)])
          kept))

;; A named let that recurs, bracketed bindings.
(define (summarize items)
        (let loop
          ([rest items] [total 0])
          (cond
            [(null? rest) total]
            [else
             (loop (cdr rest) (+ total (string-length (car rest))))])))

;; Non-tail named-let recursion: correct, and never to be reported.
(define (interleave xs sep)
        (let join
          ([rest xs])
          (cond
            [(null? rest) '()]
            [(null? (cdr rest)) (list (car rest))]
            [else (cons (car rest) (cons sep (join (cdr rest))))])))

;; `begin` sequencing two effects.
(define (log-and-return value) (begin (displayln value) value))

;; `eq?` on a symbol, which Racket and R7RS both guarantee.
(define (marker? value) (eq? value 'marker))

;; `memq`/`assq` appear here too so the corpus exercises the heads, even
;; though `scheme-memq-assq-literal-key` is Scheme-only and never fires on
;; Racket at all.
(define (flag? name flags) (memq name flags))

(define (lookup key table) (assq key table))

;; `eqv?` on a character, and `=` on a number: the two correct spellings.
(define (newline-char? c) (eqv? c #\newline))

(define (zero-length? n) (= n 0))

;; `let*` with independent literal bindings would be reported -- so this one is
;; deliberately written as the `let` it should be.
(define (defaults)
        (let ([width 80]
              [height 24])
          (cons width height)))

;; Quoted data mentioning every anchored head.
(define samples
        '((begin x)
          (let* ([a 1]
                 [b 2])
            b)
          (let loop
            ()
            (void))
          (memq 5 x)))

;; A vector constant that looks like a call.
(define heads #(begin let let* memq assq))

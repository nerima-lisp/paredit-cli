;;; Realistic, correct R7RS Scheme that exercises every head this package
;;; anchors on -- `begin`, `let`, `let*`, `memq`, `assq` -- without earning a single
;;; finding. If a rule here starts firing, it has learned to complain about
;;; ordinary code.

(define-library (example queue)
  (export make-queue queue-push! queue-pop! queue-empty?)
  (import (scheme base) (scheme write))

  ;; A library-declaration `begin`, which is not the expression operator and
  ;; must never be unwrapped even when it holds one form.
  (begin
    (define-record-type <queue>
      (make-raw-queue front back)
      queue?
      (front queue-front set-queue-front!)
      (back queue-back set-queue-back!))))

(define (make-queue)
  (make-raw-queue '() '()))

(define (queue-empty? q)
  (and (null? (queue-front q))
       (null? (queue-back q))))

;; `begin` sequencing two effects: doing its job.
(define (queue-push! q value)
  (begin
    (set-queue-back! q (cons value (queue-back q)))
    q))

;; A named `let` that actually recurs, in tail position.
(define (queue-length q)
  (let loop ((items (queue-front q)) (total 0))
    (if (null? items)
        total
        (loop (cdr items) (+ total 1)))))

;; A named `let` that recurs in NON-tail position. Correct, idiomatic Scheme:
;; R7RS 3.5 guarantees tail calls, it does not require them.
(define (queue->list q)
  (let build ((items (queue-back q)))
    (if (null? items)
        '()
        (cons (car items) (build (cdr items))))))

;; `let*` that genuinely uses its sequential scope.
(define (queue-pop! q)
  (let* ((front (queue-front q))
         (head (if (pair? front) (car front) #f))
         (rest (if (pair? front) (cdr front) '())))
    (set-queue-front! q rest)
    head))

;; `let*` whose initializers are calls: independent by name, but ordered by
;; effect, so converting to `let` would be wrong.
(define (read-pair)
  (let* ((first (read))
         (second (read)))
    (cons first second)))

;; `eq?` on the types R7RS 6.1 guarantees it for.
(define (symbolic? value)
  (or (eq? value 'none)
      (eq? value '())
      (eq? value #f)))

;; `memq`/`assq` on symbols, which is the case they exist for and which
;; R7RS 6.1 guarantees; and `memv`/`assv` where the key is a number or a
;; character, which is what this package asks for.
(define (flag? name flags)
  (memq name flags))

(define (lookup key table)
  (assq key table))

(define (code-known? code codes)
  (memv code codes))

(define (entry-for n table)
  (assv n table))

;; `eqv?` where the operand is a number or a character, which is what this
;; package asks for.
(define (digit-value c)
  (cond ((eqv? c #\0) 0)
        ((eqv? c #\1) 1)
        (else #f)))

;; Quoted data that mentions every head, and must be invisible to all of them.
(define syntax-samples
  '((begin x)
    (let* ((a 1) (b 2)) b)
    (let loop ((i 0)) i)
    (memq 5 x)))

;; A vector constant whose first element is spelled like an operator. The
;; reader makes this a self-evaluating vector (R7RS 4.1.2), not a call.
(define operator-names #(begin let let* memq assq))

;; A macro template whose named let recurs.
(define-syntax while
  (syntax-rules ()
    ((_ test body ...)
     (let loop ()
       (when test
         body ...
         (loop))))))

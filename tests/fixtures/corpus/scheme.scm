;;;; Scheme shapes: define-values, named let, syntax-rules, and block comments.

(define-library (paredit corpus)
  (export walk fold-tree)
  (import (scheme base) (scheme write))
  (begin

    (define-syntax swap!
      (syntax-rules ()
        ((_ a b)
         (let ((tmp a))
           (set! a b)
           (set! b tmp)))))

    (define-values (numerator denominator)
      (values 22 7))

    (define (fold-tree proc seed tree)
      (let loop ((nodes tree) (acc seed))
        (cond
          ((null? nodes) acc)
          ((pair? (car nodes))
           (loop (cdr nodes) (loop (car nodes) acc)))
          (else (loop (cdr nodes) (proc (car nodes) acc))))))

    (define (walk tree)
      (let*-values (((evens odds) (partition even? tree))
                    ((total) (apply + tree)))
        (list evens odds total)))

    #| A block comment, with a (paren) and a "string" inside it. |#

    ;; Datum comment: the next datum is not read at all.
    (define ignored (list 1 #;(2 3) 4))))

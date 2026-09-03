#lang racket/base

;; Racket: `#lang`, structs, keyword arguments, bracketed binding clauses,
;; `for` comprehensions, and `match`. Square brackets where Racket style uses
;; them (binding clauses and `cond` arms), parentheses everywhere else.

(require racket/list
         racket/match
         racket/string)

(provide (struct-out account)
         classify
         summarise
         transfer)

(struct account (id [balance #:mutable]) #:transparent)

(define default-options
  (hash 'retries 3
        'timeout 0.5))

(define (make-accounts ids)
  (for/list ([id (in-list ids)])
    (account id 0)))

(define (classify n)
  (cond
    [(negative? n) 'negative]
    [(zero? n) 'zero]
    [(> n 100) 'large]
    [else 'positive]))

(define (transfer from to amount
                  #:note [note ""]
                  #:allow-overdraft? [overdraft? #f])
  (unless (or overdraft? (>= (account-balance from) amount))
    (error 'transfer "insufficient funds in ~a" (account-id from)))
  (set-account-balance! from (- (account-balance from) amount))
  (set-account-balance! to (+ (account-balance to) amount))
  (list (account-id from) (account-id to) amount note))

(define (summarise accounts)
  (define total
    (for/sum ([a (in-list accounts)])
      (account-balance a)))
  (define labels
    (for/list ([a (in-list accounts)]
               #:unless (zero? (account-balance a)))
      (format "~a:~a" (account-id a) (account-balance a))))
  (string-join labels ", " #:after-last (format " (total ~a)" total)))

(define (walk tree)
  (let loop ([nodes tree]
             [acc '()])
    (match nodes
      ['() (reverse acc)]
      [(cons (? list? head) tail) (loop tail (append (walk head) acc))]
      [(cons head tail) (loop tail (cons head acc))])))

(define (render items)
  `(ul ,@(map (lambda (item) `(li ,(format "~a" item))) items)))

(module+ test
  (require rackunit)
  (check-equal? (classify -1) 'negative)
  (check-equal? (walk '(1 (2 3) 4)) '(1 2 3 4)))

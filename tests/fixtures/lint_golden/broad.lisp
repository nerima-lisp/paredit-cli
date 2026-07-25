;; broad.lisp — one triggering form per `inspect lint` rule (all 134), so the
;; golden text/json/sarif/fix-plan/fix outputs pin today's behavior for every
;; rule at once. Each line is independent and self-contained; forms are not
;; meant to be loaded/evaluated, only parsed and linted.

;; --- fixable rules (mirrors the fixable_rules_match_the_fix_engine guard
;; --- test in src/presentation/cli/lint_report/workflow.rs) ---
(list '5)                                           ; redundant-quote
(progn only)                                        ; redundant-progn
(progn a (progn b c))                               ; nested-progn (the inner progn)
(when q (progn s t))                                ; redundant-body-progn
(let () (ela) (elb))                                ; empty-let
(if c d nil)                                        ; redundant-if-nil
(funcall #'g m)                                     ; redundant-funcall
(the t whatever)                                    ; redundant-the
(funcall (lambda (fx) fx) 9)                        ; funcall-lambda
(mapcar #'(lambda (sq) sq) sqs)                     ; sharp-quoted-lambda
(identity h)                                        ; redundant-identity
(cons e nil)                                        ; cons-to-list
(reverse (reverse dr))                              ; double-reverse
(append (list al) ar)                               ; append-list-to-cons
(format nil "~A" fs)                                ; format-to-string
(format t "~%")                                     ; format-newline
(floor fq 1)                                        ; redundant-divisor
(- 0 amt)                                           ; verbose-negation
(list* la lb)                                       ; list-star-to-cons
(values-list (list va vb))                          ; values-list-of-list
(prog1 (p1x))                                       ; redundant-prog1
(subseq sz 0)                                       ; subseq-zero
(car (nthcdr cn cx))                                ; car-nthcdr
(car (reverse crx))                                 ; car-reverse
(append anx nil)                                    ; append-nil
(multiple-value-list (values mva mvb))              ; multiple-value-list-of-values
(typep tpx 'string)                                 ; typep-predicate
(coerce ctx t)                                      ; coerce-to-t
(gethash gdk gdh nil)                               ; gethash-default
(make-hash-table :test 'eql)                        ; make-hash-table-test
(let* ((a 1)) a)                                    ; redundant-let-star
(cond (ok (run)))                                   ; single-clause-cond
(cond (t (r1) (r2)))                                ; cond-t-clause
(incf tally 1)                                      ; explicit-step-delta
(incf nsd -3)                                       ; negated-step-delta
(return-from blk nil)                               ; explicit-nil-return
(multiple-value-bind (mv) (vals) mv)                ; single-value-bind
(or za (or pb qc))                                  ; nested-boolean
(when wa (when wb (wc)))                            ; nested-when
(unless ua (unless ub (uc)))                        ; nested-unless
(and x)                                             ; single-operand-boolean
(append solo)                                       ; single-operand-list-op
(* x)                                               ; single-operand-arithmetic
(when (not r) y)                                    ; negated-when-unless
(if p q)                                            ; one-armed-if
(setf ctr (1+ ctr))                                 ; manual-incf
(setf lst (cons e lst))                             ; manual-push
(setf st (adjoin e st))                             ; manual-pushnew
(car (cdr z))                                       ; nested-cxr
(nth 0 zs)                                          ; nth-constant-index
(nthcdr 0 nz)                                       ; nthcdr-zero
(nthcdr 2 ns)                                       ; nthcdr-small-index
(apply #'g (list m))                                ; redundant-apply
(find ret lst :test #'eql)                          ; redundant-eql-test
(find rsz lst :start 0)                             ; redundant-start-zero
(find ren lst :end nil)                             ; redundant-end-nil
(find rfe lst :from-end nil)                        ; redundant-from-end-nil
(remove rcn lst :count nil)                         ; redundant-count-nil
(string= (string-downcase sa) (string-downcase sb)) ; string-case-fold
(char= (char-downcase ca) (char-downcase cb))       ; char-case-fold
(string-upcase (string-downcase nsc))               ; nested-string-case
(code-char (char-code ccc))                         ; code-char-char-code
(last ldc 1)                                        ; last-default-count
(butlast bdc 1)                                     ; butlast-default-count
(make-list mde :initial-element nil)                ; make-list-default-element
(parse-integer pir :radix 10)                       ; parse-integer-default-radix
(getf gdn :key nil)                                 ; getf-default-nil
(make-array madk :adjustable nil)                   ; make-array-default-keyword
(char-upcase (char-downcase ncc))                   ; nested-char-case
(list* lsn1 lsn2 nil)                               ; list-star-nil
(sort rik #'< :key #'identity)                      ; redundant-identity-key
(= tally 0)                                         ; sign-comparison
(not (< a b))                                       ; negated-comparison
(if (not c) a b)                                    ; negated-if
(if iv iv jv)                                       ; if-to-or
(if iw nil t)                                       ; if-not
(if iu nil (iue))                                   ; if-to-unless
(prog2 (p2a) (p2b))                                 ; prog2-to-progn
(handler-case (hcx))                                ; handler-case-no-clauses
(unwind-protect (upx))                              ; unwind-protect-no-cleanup
(+ osa 1)                                           ; one-step-arithmetic
(if t on off)                                       ; constant-if-test
(when t (bd))                                       ; constant-when-test
(and p t q)                                         ; redundant-boolean-identity
(and (not p) (not q))                               ; de-morgan
(equal w nil)                                       ; nil-comparison
(eq n 7)                                            ; eq-number-comparison
(eq c #\a)                                          ; eq-char-comparison
(if a b c d)                                        ; if-arity (NOT fixable)

;; --- report-only rules (no auto-fix), one form per rule ---
(setq x x)                                          ; self-assignment
(defun reset () (setf total 0 total 1))             ; duplicate-setf-places
(setf (slot a) 1 (slot b))                          ; setf-arity
(setq (car x) 5)                                    ; setq-non-variable
(incf counter 1 2)                                  ; modify-macro-arity
(defun scale (factor x factor) (* factor x))        ; duplicate-parameters
(defun dupkw (&optional x &optional y) x)           ; duplicate-lambda-list-keyword
(defun kworder (&key a &optional b) a)              ; lambda-list-keyword-order
(when (eq status status) 1)                         ; self-comparison
(defun ready? (x) (eq (compute x) t))               ; t-comparison
(if test (foo x) (foo x))                           ; identical-if-branches
(the fixnum)                                        ; the-arity
(eq x)                                              ; equality-arity
(gethash key)                                       ; accessor-arity
(case x (:a 1) (:b 2) (:a 3))                       ; duplicate-case-keys
(case sym ('apple :fruit) ('carrot :veg) (t :x))    ; quoted-case-key
(case x (nil 1) (t 2))                              ; case-nil-key
(typecase x (nil 1) (t 2))                          ; typecase-nil-key
(case x (1 :one) 2 :two)                            ; malformed-case-clause
(case x (1 :one) (t :def) (2 :two))                 ; unreachable-case-clause
(ecase x (1 :one) (t :default))                     ; exhaustive-case-otherwise
(cond ((foo) 1) ((bar) 2) ((foo) 3))                ; duplicate-cond-tests
(cond ((foo) 1) (t 2) ((bar) 3))                    ; unreachable-cond-clause
(cond ((plusp x) :pos) ((minusp x) :neg) :zero)     ; malformed-cond-clause
(let ((x 1) (y 2) (x 3)) x)                         ; duplicate-let-bindings
(let ((x 1 y 2)) (+ x y))                           ; malformed-let-binding
(let ((nil 1) (:status :ok)) :status)               ; binds-constant
(dolist (x) (print x))                              ; malformed-iteration-spec
(eval-when (:compile-toplevel :executee) 1)         ; eval-when-situation
(or x y x)                                          ; duplicate-boolean-operands
(and (ready a) nil (finalize b))                    ; dead-boolean-operand
(when (eql name "root") 1)                          ; eql-string-comparison
(when (eql p '(:a :b)) 1)                           ; eql-list-comparison
(defun eqlsearch (items) (member "x" items))        ; eql-search-literal
(defun singlearg (x) (when (< x) (go)))             ; single-arg-comparison
(defun missingdest (x) (format "~a~%" x))           ; format-missing-destination
(defun litplace () (incf 5))                        ; literal-place
(defun destrlit () (nreverse '(a b c)))              ; destructive-literal
(defun charopstr (c) (when (char= c "a") :hit))     ; char-op-string
(defun emptybody (ready) (when ready))              ; empty-body
(defun identarith (x) (+ x 0))                      ; identity-arithmetic
(/ x 0)                                             ; zero-divisor
(make-instance 'c :x 1 :x 2)                        ; duplicate-keyword
(defpackage :app (:export 'foo))                    ; defpackage-quoted
(incf counter 0)                                    ; step-zero

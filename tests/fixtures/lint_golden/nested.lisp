;; nested.lisp — nested/overlapping forms that exercise fix ordering and
;; fixpoint convergence (a fix on an outer form unlocking a fix on what the
;; rewrite exposes, across several `--fix` passes within one run).

(defun converges-through-progn-then-boolean ()
  (progn (or x)))

(defun triple-nested-when (a b c)
  (when a
    (when b
      (when c
        (act)))))

(defun triple-nested-unless (a b c)
  (unless a
    (unless b
      (unless c
        (act)))))

(defun deeply-nested-progn ()
  (progn
    a
    (progn
      b
      (progn c d))
    e))

(defun overlapping-cxr (x)
  (car (cdr (cdr x))))

(defun redundant-body-progn-inside-when (flag)
  (when flag
    (progn
      (step-one)
      (step-two))))

(defun nested-boolean-mixed (p q r)
  (and p (and q (or r (or p q)))))

(defun nested-quote-inside-progn ()
  (progn '5))

(defun stacked-redundant-lets ()
  (let* ((a 1))
    (let () (progn a))))

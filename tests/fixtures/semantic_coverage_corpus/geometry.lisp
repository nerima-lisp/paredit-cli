(in-package :cl-user)

(defconstant +pi-approx+ 3.14159)

(defun circle-area (radius)
  (let ((r2 (* radius radius)))
    (* +pi-approx+ r2)))

(defun rectangle-area (width height)
  (let ((area (* width height)))
    area))

(defun square-perimeter (side)
  (let ((sides 4))
    (* sides side)))

(defun unfoldable-total (items)
  ;; `reduce` is not a registered folding operator, so `total`'s initial form
  ;; does not fold: this fixture pins a nonzero `initial_form_not_constant`
  ;; count alongside the resolvable bindings above, exercising both sides of
  ;; the non-resolution breakdown.
  (let ((total (reduce #'+ items)))
    total))

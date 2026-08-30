(in-package :cl-user)

(defconstant +max-retries+
  3)

(defun clamp (value low high)
  (let ((lower (min value high)))
    (max lower low)))

(defun retry-budget ()
  (let ((budget +max-retries+))
    (* budget 2)))

(defun greet (name)
  (let ((prefix "Hello, "))
    (concatenate 'string prefix name)))

;;;; Reader syntax that historically confused span-based tooling.
;;;;
;;;; Every form here is legal Common Lisp. The corpus test asserts only that
;;;; parsing is lossless, formatting converges, and the tree's own paths
;;;; resolve -- not that anything in particular is understood.
(in-package #:paredit-corpus)

;;; Feature expressions. `#+`/`#-` suppress the *following form*, which means
;;; a naive reader can lose track of how many forms a file has.
#+sbcl (defun implementation () :sbcl)

#-sbcl (defun implementation () :other)

#+(and sbcl (not win32)) (defun posix-only () t)

#+#.(cl:if (cl:find-package "ASDF") '(:and) '(:or)) (defun asdf-present () t)

;;; Read-time evaluation.
(defconstant +built-at-read-time+
  #.(* 6 7))

;;; Circular and shared structure literals.
(defparameter *shared*
  '#1=(a b . #1#))

(defparameter *pair*
  (list '#2=(x) '#2#))

;;; Characters whose names contain delimiters. A reader that scans for `)`
;;; without understanding `#\` closes the list here.
(defparameter *delimiters*
  (list #\( #\) #\" #\; #\Space #\Newline #\\))

;;; Block comments, including nested ones.
#|
  (this is not code)
  #| and this is a nested block comment |#
|#
(defun after-block-comment ()
  :ok)

;;; Strings containing everything that looks like structure.
(defparameter *tricky-strings*
  (list ")" "(" ";; not a comment" "#|" "|#" "\\" "\"" "a
multi-line
string"))

;;; Vertical-bar symbols: the delimiters inside are part of the name.
(defun |weird name with (parens) and ;semicolons| ()
  :ok)

(defparameter |a|
  '|b c|)

;;; Vectors, bit vectors, and pathnames.
(defparameter *vector*
  #(1 2 3))

(defparameter *bits*
  #*10110)

(defparameter *path*
  #p"/tmp/corpus.lisp")

;;; Quasiquotation nested two deep.
(defmacro nested-template (name)
  `(defmacro ,name (inner)
     `(list ',inner ,@(list 1 2 3))))

;;; Dotted pairs and the empty list.
(defparameter *dotted*
  '(a . b))

(defparameter *nothing*
  '())

;;; Numeric literal dispatches. `#c` is CLHS 2.4.8.11 and was the one ANSI
;;; dispatch this reader did not know until a Quicklisp corpus run found it.
(defparameter *complex*
  (list #C(0.0 0.0) #C(1.5 -2.0) #c(1/2 3/4)))

(defparameter *radix*
  (list #b1011 #o755 #xdeadBEEF #16r1F #2r1010))

(defparameter *array*
  #2A((1 2 3) (4 5 6)))

(defparameter *struct*
  #S(paredit-corpus-point :x 1 :y 2))

(defparameter *ratio*
  (list 1/3 -22/7))

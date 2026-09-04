;;; elisp.el --- Emacs Lisp corpus fixture  -*- lexical-binding: t; -*-

;;; Commentary:

;; Emacs Lisp reader syntax that differs from Common Lisp: `?a` characters,
;; `#'` in different positions, and docstrings in every definition form.

;;; Code:

(require 'cl-lib)

(defgroup paredit-corpus nil
  "A corpus fixture."
  :group 'tools
  :prefix "paredit-corpus-")

(defcustom paredit-corpus-delay 0.5
  "Seconds to wait."
  :type 'number
  :group 'paredit-corpus)

(defvar paredit-corpus--cache nil
  "Internal cache.")

(defconst paredit-corpus-characters (list ?\( ?\) ?\; ?\" ?\\ ?\s ?\n ?a)
  "Characters whose reader syntax embeds delimiters.")

;;;###autoload
(defun paredit-corpus-run (items &optional predicate)
  "Run over ITEMS, keeping those matching PREDICATE."
  (interactive)
  (let ((keep (or predicate #'identity))
        (result '()))
    (dolist (item items (nreverse result))
      (when (funcall keep item)
        (push item result)))))

(cl-defstruct (paredit-corpus-node (:constructor paredit-corpus-node-create))
  name children)

(defun paredit-corpus-walk (node)
  "Walk NODE depth first."
  (pcase node
    ((pred null) nil)
    ((cl-struct paredit-corpus-node name children)
     (cons name (mapcar #'paredit-corpus-walk children)))
    (_ (list node))))

(provide 'elisp)
;;; elisp.el ends here

;;;; Depth, iteration forms, and the `loop` sublanguage.

(in-package #:paredit-corpus)

(defun deeply-nested (x)
  (if (> x 0)
      (if (> x 1)
          (if (> x 2)
              (if (> x 3)
                  (if (> x 4)
                      (if (> x 5)
                          (if (> x 6)
                              (if (> x 7)
                                  (if (> x 8) :nine :eight)
                                  :seven)
                              :six)
                          :five)
                      :four)
                  :three)
              :two)
          :one)
      :zero))

(defun tabulate (rows)
  (loop for row in rows
        for index from 0
        with total = 0
        when (evenp index)
          collect (list index row) into evens
        else
          collect (list index row) into odds
        do (incf total (length row))
        finally (return (values evens odds total))))

(defun nested-bindings ()
  (let ((a 1))
    (let* ((b (1+ a))
           (c (* b b)))
      (flet ((scale (n) (* n c)))
        (labels ((recurse (n acc)
                   (if (zerop n) acc (recurse (1- n) (scale acc)))))
          (multiple-value-bind (quotient remainder) (floor (recurse 3 1) 7)
            (destructuring-bind (&key (base 10) &allow-other-keys) '(:base 16)
              (list quotient remainder base))))))))

(defun resource-shapes (path)
  (with-open-file (stream path :direction :input :if-does-not-exist nil)
    (unwind-protect
         (restart-case (read stream nil :eof)
           (use-value (value) :report "Use a value" value)
           (skip () :report "Skip the file" nil))
      (when stream (close stream)))))

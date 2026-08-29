;;;; CLOS shapes with long lambda lists and deep nesting.
(in-package #:paredit-corpus)

(defclass account ()
  ((identifier :initarg :identifier :reader account-identifier :type string)
   (balance :initarg
            :balance
            :accessor
            account-balance
            :initform
            0
            :type
            integer)
   (history :initarg :history :accessor account-history :initform '()))
  (:documentation "An account, with a deliberately long slot list."))

(defgeneric transfer (from to amount &key currency note allow-overdraft))

(defmethod transfer ((from account) (to account)
                                    (amount integer)
                                    &key
                                    (currency :usd)
                                    note
                                    (allow-overdraft nil))
  (declare (ignore currency))
  (when (and (not allow-overdraft) (< (account-balance from) amount))
    (error 'insufficient-funds :account from :requested amount))
  (decf (account-balance from) amount)
  (incf (account-balance to) amount)
  (push (list :amount amount :note note) (account-history from)))

(defmethod transfer :before ((from account) (to account) (amount integer) &key)
  (assert (plusp amount) (amount) "transfer amount must be positive"))

(defmethod transfer :around ((from account) (to account) (amount integer) &key)
  (handler-case (call-next-method)
    (error (condition)
      (warn "transfer failed: ~a" condition)
      nil)))

(define-condition insufficient-funds
  (error)
  ((account :initarg :account :reader insufficient-funds-account)
   (requested :initarg :requested :reader insufficient-funds-requested))
  (:report
   (lambda (condition stream)
     (format stream
             "~a cannot cover ~d"
             (insufficient-funds-account condition)
             (insufficient-funds-requested condition)))))

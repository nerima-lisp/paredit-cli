#+(and sbcl (not win32)) (defun x () t)
#-sbcl (defun x () nil)

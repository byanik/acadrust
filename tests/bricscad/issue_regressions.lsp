(vl-load-com)

(defun ar:emit (key value)
  (write-line (strcat key "\t" (vl-prin1-to-string value)) ar:report))

(defun ar:check (key actual expected)
  (ar:emit key (list (if (equal actual expected 0.00000001) "PASS" "FAIL") actual expected)))

(defun ar:validate-modern (/ data sel ent tableobj sty dict merged r0 r1 c0 c1 solidobj)
  (foreach item '(("ANGBASE" 1.5707963267948966) ("ANGDIR" 1) ("FILLETRAD" 2.5)
                  ("CHAMFERA" 1.0) ("CHAMFERB" 2.0) ("ELEVATION" -3.0) ("THICKNESS" 4.0))
    (ar:check (strcat "issue127." (car item)) (getvar (car item)) (cadr item)))
  (setq data (tblsearch "LTYPE" "Dashed"))
  (ar:check "issue128.dash-count" (cdr (assoc 73 data)) 2)
  (foreach item data
    (if (= (car item) 74) (ar:check "issue128.complex-flag" (cdr item) 0)))
  (setq sel (ssget "_X" '((0 . "3DSOLID"))))
  (ar:check "issue129.solid-count" (if sel (sslength sel) 0) 1)
  (if sel
    (progn
      (setq solidobj (vlax-ename->vla-object (ssname sel 0)))
      (ar:check "issue129.volume" (vl-catch-all-apply 'vla-get-Volume (list solidobj)) 24.0)))
  (setq sel (ssget "_X" '((0 . "ACAD_TABLE"))))
  (ar:check "issue130.table-count" (if sel (sslength sel) 0) 1)
  (if sel
    (progn
      (setq tableobj (vlax-ename->vla-object (ssname sel 0)))
      (ar:emit "issue130.rows" (vl-catch-all-apply 'vla-get-Rows (list tableobj)))
      (ar:emit "issue130.columns" (vl-catch-all-apply 'vla-get-Columns (list tableobj)))
      (setq merged (vl-catch-all-apply 'vla-IsMergedCell (list tableobj 0 0 'r0 'r1 'c0 'c1)))
      (ar:check "issue130.is-merged" merged :vlax-true)
      (if (= merged :vlax-true)
        (ar:check "issue130.merge-range" (list r0 r1 c0 c1) '(0 0 0 2)))))
  (setq dict (dictsearch (namedobjdict) "ACAD_TABLESTYLE"))
  (if dict
    (progn
      (setq sty (dictsearch (cdr (assoc -1 dict)) "Standard"))
      (ar:check "issue130.title-suppressed" (cdr (assoc 280 sty)) 1)
      (ar:check "issue130.header-suppressed" (cdr (assoc 281 sty)) 0)
      (ar:emit "issue130.style-dxf" sty)))
  (setq sel (ssget "_X" '((0 . "LINE"))))
  (if sel
    (progn
      (setq ar:named-color nil)
      (repeat (sslength sel)
        (setq ent (ssname sel 0) data (entget ent))
        (if (assoc 430 data) (setq ar:named-color (cdr (assoc 430 data))))
        (ssdel ent sel))
      (ar:check "issue132.named-color" ar:named-color "MYBOOK$MYCOLOR")))
  (setq ent (tblobjname "LAYER" "XDATA132"))
  (ar:check "issue132.layer-xdata"
    (if ent (assoc -3 (entget ent '("REGRESSION132"))) nil)
    '(-3 ("REGRESSION132" (1000 . "table record payload") (1070 . 132)))))

(defun ar:run (report-path modern native-path / *error* sel text doc dxf-result)
  (defun *error* (message)
    (if ar:report (progn (ar:emit "ERROR" message) (close ar:report) (setq ar:report nil)))
    (princ))
  (setq ar:report (open report-path "w"))
  (ar:emit "DWGNAME" (getvar "DWGNAME"))
  (setvar "CMDECHO" 1)
  (setvar "FILEDIA" 0)
  (setvar "EXPERT" 5)
  (setvar "AUDITCTL" 1)
  (command "_.AUDIT" "_No")
  (ar:emit "AUDIT-COMPLETE" T)
  (if modern
    (ar:validate-modern)
    (progn
      (setq sel (ssget "_X" '((0 . "TEXT"))))
      (ar:check "issue131.text-count" (if sel (sslength sel) 0) 1)
      (if sel
        (progn
          (setq text (cdr (assoc 1 (entget (ssname sel 0)))))
          (ar:emit "issue131.text" text)
          (ar:emit "issue131.character-codes" (vl-string->list text))
          (ar:check "issue131.expected-text" text
            (if (wcmatch (strcase (getvar "DWGNAME")) "JOHAB*") "\\U+AC00" (chr 233)))))
      (ar:emit "issue131.code-page" (getvar "DWGCODEPAGE"))))
  (if native-path
    (progn
      (setq doc (vla-get-ActiveDocument (vlax-get-acad-object)))
      (vla-SaveAs doc native-path 48)
      (setq dxf-result (vl-catch-all-apply 'vla-SaveAs (list doc (strcat native-path ".dxf") 49)))
      (ar:emit "NATIVE-DXF-SAVE" dxf-result)
      (ar:emit "NATIVE-SAVE" native-path)))
  (ar:emit "COMPLETE" T)
  (close ar:report)
  (setq ar:report nil)
  (command "_.QUIT" "_No")
  (princ))

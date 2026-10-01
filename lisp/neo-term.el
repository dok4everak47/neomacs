;;; neo-term.el --- GPU-accelerated terminal emulator for Neomacs -*- lexical-binding: t -*-

;; Copyright (C) 2026 Neomacs Contributors
;; License: GPL-3.0-or-later

;;; Commentary:

;; neo-term provides a GPU-accelerated terminal emulator backed by
;; rio-vt + portable-pty + wgpu, integrated into the neomacs display engine.
;;
;; Three display modes:
;;   - Window mode: terminal fills a regular Emacs window/buffer
;;   - Inline mode: terminal embedded inline in a buffer (like images)
;;   - Floating mode: terminal rendered as a floating overlay
;;
;; Usage:
;;   M-x neo-term          -- open a terminal in the current window
;;   M-x neo-term-floating -- open a floating terminal overlay

;;; Code:

(require 'cl-lib)

(defgroup neo-term nil
  "GPU-accelerated terminal emulator."
  :group 'terminals
  :prefix "neo-term-")

(defcustom neo-term-shell nil
  "Shell program to run.  nil means use `explicit-shell-file-name' or $SHELL."
  :type '(choice (const :tag "Default" nil) string)
  :group 'neo-term)

(defcustom neo-term-default-cols 80
  "Default terminal width in columns."
  :type 'integer
  :group 'neo-term)

(defcustom neo-term-default-rows 24
  "Default terminal height in rows."
  :type 'integer
  :group 'neo-term)

(defvar neo-term--terminals (make-hash-table :test 'eql)
  "Hash table mapping terminal-id to terminal info plists.")

(defvar neo-term--next-buffer-num 1
  "Next buffer number for naming.")

;; These are Rust builtins routed through the active GUI display host.
(declare-function neomacs-terminal-create "neovm-core"
                  (cols rows mode &optional shell))
(declare-function neomacs-terminal-write "neovm-core"
                  (terminal-id string))
(declare-function neomacs-terminal-resize "neovm-core"
                  (terminal-id cols rows))
(declare-function neomacs-terminal-destroy "neovm-core"
                  (terminal-id))
(declare-function neomacs-terminal-set-float "neovm-core"
                  (terminal-id x y opacity))
(declare-function neomacs-terminal-get-text "neovm-core"
                  (terminal-id))

(defun neo-term--shell-path ()
  "Return shell program to use."
  (or neo-term-shell
      (bound-and-true-p explicit-shell-file-name)
      (getenv "SHELL")
      "/bin/sh"))

(defun neo-term--create (cols rows mode &optional shell)
  "Create a terminal.  MODE is 0=Window, 1=Inline, 2=Floating.
Returns terminal ID or nil on failure."
  (let ((shell-path (or shell (neo-term--shell-path))))
    (condition-case err
        (let ((id (neomacs-terminal-create cols rows mode shell-path)))
          (when (and id (> id 0))
            (puthash id (list :id id :cols cols :rows rows :mode mode
                              :shell shell-path)
                     neo-term--terminals)
            id))
      (error
       (message "neo-term: failed to create terminal: %s" (error-message-string err))
       nil))))

(defun neo-term--destroy (terminal-id)
  "Destroy a terminal."
  (when terminal-id
    (ignore-errors (neomacs-terminal-destroy terminal-id))
    (remhash terminal-id neo-term--terminals)))

(defun neo-term--write (terminal-id string)
  "Send STRING to the terminal."
  (when (and terminal-id string)
    (neomacs-terminal-write terminal-id string)))

(defun neo-term--resize (terminal-id cols rows)
  "Resize a terminal."
  (when terminal-id
    (neomacs-terminal-resize terminal-id cols rows)))

;;; Major mode

(defvar neo-term-mode-map
  (let ((map (make-sparse-keymap)))
    ;; Suppress default self-insert so stray keys don't modify the buffer
    (suppress-keymap map t)

    ;; All printable ASCII characters (space=32 through ~=126)
    (dotimes (i 95)
      (define-key map (string (+ i 32)) #'neo-term-send-key))

    ;; Standard editing keys
    (define-key map (kbd "RET") #'neo-term-send-return)
    (define-key map (kbd "DEL") #'neo-term-send-backspace)
    (define-key map (kbd "<backspace>") #'neo-term-send-backspace)
    (define-key map (kbd "TAB") #'neo-term-send-tab)
    (define-key map [escape] #'neo-term-send-escape)

    ;; Arrow keys, navigation keys, and function keys (send ANSI sequences)
    (dolist (key '("<up>" "<down>" "<left>" "<right>"
                   "<home>" "<end>" "<prior>" "<next>"
                   "<insert>" "<delete>"
                   "<f1>" "<f2>" "<f3>" "<f4>" "<f5>" "<f6>"
                   "<f7>" "<f8>" "<f9>" "<f10>" "<f11>" "<f12>"))
      (define-key map (kbd key) #'neo-term-send-special))

    ;; Control keys sent directly to terminal (C-a=1 .. C-z=26)
    ;; Skip C-c(3)=our prefix, C-i(9)=TAB, C-m(13)=RET
    (dotimes (i 26)
      (let ((ctrl-char (1+ i)))
        (unless (memq ctrl-char '(3 9 13))
          (define-key map (vector ctrl-char) #'neo-term-send-ctrl))))

    ;; C-c prefix for Emacs-level commands
    (define-key map (kbd "C-c C-c") #'neo-term-send-ctrl-c)
    (define-key map (kbd "C-c C-d") #'neo-term-send-ctrl-d)
    (define-key map (kbd "C-c C-z") #'neo-term-send-ctrl-z)
    (define-key map (kbd "C-c C-\\") #'neo-term-send-ctrl-backslash)
    (define-key map (kbd "C-c C-q") #'neo-term-quit)
    map)
  "Keymap for `neo-term-mode'.")

(define-derived-mode neo-term-mode fundamental-mode "NeoTerm"
  "Major mode for neo-term GPU terminal buffers.

Drag with the mouse (or hold Shift over a program that reports mouse
events) to select text; releasing the button copies the selection to the
kill ring.

\\{neo-term-mode-map}"
  :group 'neo-term
  (setq-local buffer-read-only t)
  (setq-local truncate-lines t)
  (setq-local neo-term--id nil)
  (add-hook 'kill-buffer-hook #'neo-term--kill-buffer-terminal nil t))

(defvar-local neo-term--id nil
  "Terminal ID for this buffer.")

(defun neo-term--kill-buffer-terminal ()
  "Destroy the terminal owned by the buffer being killed."
  (when neo-term--id
    (let ((id neo-term--id))
      ;; Clear first so errors or recursive buffer cleanup cannot queue a
      ;; second destroy for the same host-owned terminal.
      (setq neo-term--id nil)
      (neo-term--destroy id))))

(defun neo-term-send-key ()
  "Send the current key to the terminal."
  (interactive)
  (when neo-term--id
    (let* ((keys (this-command-keys))
           (str (if (stringp keys) keys (string (event-basic-type last-input-event)))))
      (neo-term--write neo-term--id str))))

(defun neo-term-send-return ()
  "Send Return to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\r")))

(defun neo-term-send-backspace ()
  "Send Backspace to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\177")))

(defun neo-term-send-tab ()
  "Send Tab to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\t")))

(defun neo-term-send-escape ()
  "Send Escape to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\e")))

(defun neo-term-send-ctrl ()
  "Send control character to the terminal."
  (interactive)
  (when neo-term--id
    (neo-term--write neo-term--id (string last-input-event))))

(defun neo-term-send-special ()
  "Send special key (arrows, function keys, etc.) as ANSI escape sequence."
  (interactive)
  (when neo-term--id
    (let ((seq (neo-term--key-to-ansi last-input-event)))
      (when seq (neo-term--write neo-term--id seq)))))

(defun neo-term--key-to-ansi (key)
  "Convert Emacs KEY event symbol to ANSI escape sequence string."
  (pcase key
    ('up     "\e[A")
    ('down   "\e[B")
    ('right  "\e[C")
    ('left   "\e[D")
    ('home   "\e[H")
    ('end    "\e[F")
    ('prior  "\e[5~")
    ('next   "\e[6~")
    ('insert "\e[2~")
    ('delete "\e[3~")
    ('f1  "\eOP")
    ('f2  "\eOQ")
    ('f3  "\eOR")
    ('f4  "\eOS")
    ('f5  "\e[15~")
    ('f6  "\e[17~")
    ('f7  "\e[18~")
    ('f8  "\e[19~")
    ('f9  "\e[20~")
    ('f10 "\e[21~")
    ('f11 "\e[23~")
    ('f12 "\e[24~")))

(defun neo-term-send-ctrl-c ()
  "Send C-c to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\003")))

(defun neo-term-send-ctrl-d ()
  "Send C-d to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\004")))

(defun neo-term-send-ctrl-z ()
  "Send C-z to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\032")))

(defun neo-term-send-ctrl-backslash ()
  "Send C-\\ to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\034")))

(defun neo-term-quit ()
  "Kill the terminal and close the buffer."
  (interactive)
  (kill-buffer))

(defun neo-term--handle-exit (terminal-id)
  "Handle terminal TERMINAL-ID process exit."
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (and (eq major-mode 'neo-term-mode)
                 (eql neo-term--id terminal-id))
        (neo-term--destroy terminal-id)
        (setq neo-term--id nil)
        (let ((inhibit-read-only t))
          (goto-char (point-max))
          (insert "\n[Process exited]\n"))
        (message "neo-term: terminal %d exited" terminal-id)))))

(defun neo-term--handle-create-failed (terminal-id error)
  "Handle creation failure ERROR for TERMINAL-ID."
  ;; Retire the failed reservation through the typed Rust lifecycle.  There is
  ;; no PTY to tear down, but the renderer must acknowledge removal of the ID.
  (ignore-errors (neomacs-terminal-destroy terminal-id))
  (remhash terminal-id neo-term--terminals)
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (and (eq major-mode 'neo-term-mode)
                 (eql neo-term--id terminal-id))
        ;; The renderer owns the failed lifecycle record; clearing the local
        ;; ID prevents kill-buffer cleanup from attempting to destroy it.
        (setq neo-term--id nil)
        (let ((inhibit-read-only t))
          (goto-char (point-max))
          (insert (format "\n[Terminal creation failed: %s]\n" error)))
        (message "neo-term: terminal %d creation failed: %s"
                 terminal-id error)))))

(defun neo-term--handle-title-changed (terminal-id title)
  "Handle terminal TERMINAL-ID title change to TITLE."
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (and (eq major-mode 'neo-term-mode)
                 (eql neo-term--id terminal-id))
        (rename-buffer (format "*neo-term: %s*" title) t)))))

(defun neo-term--handle-selection (terminal-id text)
  "Copy drag-selected TEXT from TERMINAL-ID to the kill ring."
  (ignore terminal-id)
  (when (and (stringp text) (> (length text) 0))
    (kill-new text)))

(defvar neo-term-exit-functions nil
  "Functions called with a terminal ID after its child process exits.")

(defvar neo-term-create-failed-functions nil
  "Functions called with a terminal ID and renderer creation error.")

(defvar neo-term-title-changed-functions nil
  "Functions called with a terminal ID and its new title.")

(defvar neo-term-selection-functions nil
  "Functions called with a terminal ID and its drag-selected text.")

(add-hook 'neo-term-exit-functions #'neo-term--handle-exit)
(add-hook 'neo-term-create-failed-functions #'neo-term--handle-create-failed)
(add-hook 'neo-term-title-changed-functions #'neo-term--handle-title-changed)
(add-hook 'neo-term-selection-functions #'neo-term--handle-selection)

;;; Public API

;;;###autoload
(defun neo-term ()
  "Open a new GPU-accelerated terminal in the current window."
  (interactive)
  (let* ((buf-name (format "*neo-term-%d*" neo-term--next-buffer-num))
         (buf (get-buffer-create buf-name)))
    (switch-to-buffer buf)
    (neo-term-mode)
    ;; The Rust boundary captures the current buffer as the typed owner of a
    ;; Window terminal, so creation must occur after switching to BUF.
    (let ((id (neo-term--create neo-term-default-cols neo-term-default-rows
                                0))) ; mode=0 (Window)
      (unless id
        (kill-buffer buf)
        (error "Failed to create terminal"))
      (cl-incf neo-term--next-buffer-num)
      (setq-local neo-term--id id)
      (message "neo-term: terminal %d created (%dx%d)"
               id neo-term-default-cols neo-term-default-rows))))

;;;###autoload
(defun neo-term-floating (&optional x y cols rows)
  "Open a floating GPU terminal overlay.
Optional X, Y set the floating position.
Optional COLS, ROWS set the terminal size."
  (interactive)
  (let* ((cols (or cols neo-term-default-cols))
         (rows (or rows neo-term-default-rows))
         (id (neo-term--create cols rows 2))) ; mode=2 (Floating)
    (unless id
      (error "Failed to create floating terminal"))
    (when (or x y)
      (neomacs-terminal-set-float
       id (or x 100.0) (or y 100.0) 0.95))
    (message "neo-term: floating terminal %d created (%dx%d)" id cols rows)
    id))

(provide 'neo-term)
;;; neo-term.el ends here

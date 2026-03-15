 .section .data.app
     .globl app_start
  app_start:
     .incbin "./init"
     .globl app_end
  app_end:

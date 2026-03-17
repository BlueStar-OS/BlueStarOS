 .section .data.app
     .globl app_start
  app_start:
     .incbin "./cinit"
     .globl app_end
  app_end:

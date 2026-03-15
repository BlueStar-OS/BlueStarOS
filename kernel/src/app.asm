 .section .data.app
     .globl app_start
  app_start:
     .incbin "./musl_test"
     .globl app_end
  app_end:

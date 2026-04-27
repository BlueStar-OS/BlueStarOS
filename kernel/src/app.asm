 .section .data.app
     .globl app_start
  app_start:
     .incbin "./ainit"
     .globl app_end
  app_end:

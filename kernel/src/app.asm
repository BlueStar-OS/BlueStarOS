 .section .data.app
     .globl app_start
  app_start:
     .incbin "../user/target/riscv64gc-unknown-none-elf/release/cinit"
     .globl app_end
  app_end:

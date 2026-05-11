#include<stdio.h>
#include<stdlib.h>
int main() {
    printf("I will call malloc \n");
    int *a = malloc(sizeof(int)*16);
    if (!a)
    {
        fprintf(stderr,"alloc fail \n");
        return 1;
    }
    
    for (int i = 0; i < 16; i++)
    {
        a[i]=i;
    }


    for (int i = 0; i < 16; i++)
    {
        printf("I read %d \n",a[i]);
    }
    
    
    printf("Allocated memory at: %p\n", a);
    
    
    free(a);
    

    return 0;
}